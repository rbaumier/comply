//! LLM backend — semantic rule evaluation via `claude` CLI.
//!
//! Activated by `comply --with-llm`. Sends ONE `claude -p` call per file
//! with a unified prompt covering 9 semantic rules. Results cached
//! in SQLite per-project. Files are processed in parallel (default 30
//! concurrent `claude` subprocesses).

pub mod cache;
pub mod claude_cli;
pub mod extract;
pub mod pool;
pub mod unified_prompt;

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::diagnostic::Diagnostic;

/// Cache version — bump when the unified prompt changes materially.
/// All cached entries with a different version are automatically
/// invalidated (the version is part of the cache key hash).
const PROMPT_VERSION: u32 = 3;

/// Configuration for an LLM lint pass.
#[derive(Debug)]
pub struct LlmConfig {
    pub model: String,
    pub concurrency: usize,
    pub project_root: std::path::PathBuf,
}

/// A file ready for LLM evaluation — source already read and cache
/// checked. Sent to worker threads.
struct LlmJob {
    source: String,
    path: std::path::PathBuf,
    cache_key: String,
}

/// Run the unified LLM prompt on every file in parallel. Returns
/// diagnostics across all 9 rules.
pub fn lint_files(
    files: &[&crate::files::SourceFile],
    config: &LlmConfig,
) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }

    let conn = cache::open(&config.project_root)
        .context("failed to open LLM cache")?;

    // Phase 1: read files, check cache, collect jobs for cache misses.
    let mut all_diagnostics = Vec::new();
    let mut cache_hits = 0usize;
    let mut jobs = Vec::new();

    for file in files {
        let source = match std::fs::read_to_string(&file.path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if source.lines().count() < 5 {
            continue;
        }

        let key = cache::cache_key("unified-llm", PROMPT_VERSION, &source);

        if let Some(json) = cache::lookup(&conn, &key) {
            cache_hits += 1;
            if let Ok(diags) = serde_json::from_str::<Vec<Diagnostic>>(&json) {
                all_diagnostics.extend(diags);
            }
            continue;
        }

        jobs.push(LlmJob {
            source,
            path: file.path.clone(),
            cache_key: key,
        });
    }

    let cache_misses = jobs.len();
    let skipped = files.len() - cache_hits - cache_misses;

    if jobs.is_empty() {
        eprintln!(
            "comply: LLM pass — {cache_hits} cache hits, 0 misses ({skipped} files skipped)",
        );
        return Ok(all_diagnostics);
    }

    // Phase 2: process cache misses in parallel.
    let total = jobs.len();
    let done = Arc::new(AtomicUsize::new(0));
    let conn = Arc::new(Mutex::new(conn));
    let model = config.model.clone();
    let concurrency = config.concurrency.min(total);

    // Shared work queue — each thread pops jobs until empty.
    let jobs = Arc::new(Mutex::new(jobs.into_iter()));
    let results: Arc<Mutex<Vec<Diagnostic>>> = Arc::new(Mutex::new(Vec::new()));

    std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(concurrency);

        for _ in 0..concurrency {
            let jobs = Arc::clone(&jobs);
            let results = Arc::clone(&results);
            let conn = Arc::clone(&conn);
            let done = Arc::clone(&done);
            let model = model.clone();

            handles.push(s.spawn(move || {
                loop {
                    let job = {
                        let mut guard = jobs.lock().unwrap_or_else(|e| e.into_inner());
                        guard.next()
                    };
                    let Some(job) = job else { break };

                    let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
                    eprint!(
                        "\rcomply: LLM [{finished}/{total}] {}…\x1b[K",
                        job.path.display(),
                    );

                    match unified_prompt::evaluate_file(&job.source, &job.path, &model) {
                        Ok(diags) => {
                            if let Ok(json) = serde_json::to_string(&diags) {
                                let guard = conn.lock().unwrap_or_else(|e| e.into_inner());
                                let _ = cache::store(&guard, &job.cache_key, &json);
                            }
                            let mut r = results.lock().unwrap_or_else(|e| e.into_inner());
                            r.extend(diags);
                        }
                        Err(e) => {
                            eprintln!(
                                "\ncomply: LLM failed on {}: {e:#}",
                                job.path.display(),
                            );
                        }
                    }
                }
            }));
        }

        for h in handles {
            let _ = h.join();
        }
    });

    eprint!("\r\x1b[2K");
    eprintln!(
        "comply: LLM pass — {cache_hits} cache hits, {cache_misses} misses ({skipped} files skipped)",
    );

    // All threads have joined — we hold the only Arc reference.
    let thread_results = Arc::try_unwrap(results)
        .expect("all threads joined, Arc should have refcount 1")
        .into_inner()
        .unwrap_or_default();
    all_diagnostics.extend(thread_results);

    Ok(all_diagnostics)
}
