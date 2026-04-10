//! LLM backend — semantic rule evaluation via `claude` CLI.
//!
//! Activated by `comply --with-llm`. Sends ONE `claude -p` call per file
//! with a unified prompt covering all 4 semantic rules (comment quality,
//! intent naming, PII in logs, mixed abstraction levels). Results cached
//! in SQLite per-project.

pub mod cache;
pub mod claude_cli;
pub mod pool;
pub mod unified_prompt;

use anyhow::{Context, Result};
use std::sync::Mutex;

use crate::diagnostic::Diagnostic;

/// Cache version — bump when the unified prompt changes materially.
/// All cached entries with a different version are automatically
/// invalidated (the version is part of the cache key hash).
const PROMPT_VERSION: u32 = 1;

/// Configuration for an LLM lint pass.
#[derive(Debug)]
pub struct LlmConfig {
    pub model: String,
    pub concurrency: usize,
    pub project_root: std::path::PathBuf,
}

/// Run the unified LLM prompt on every file. ONE `claude` subprocess
/// per file (not per rule). Returns diagnostics across all 4 rules.
pub fn lint_files(
    files: &[&crate::files::SourceFile],
    config: &LlmConfig,
) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }

    let conn = cache::open(&config.project_root)
        .context("failed to open LLM cache")?;
    let conn = Mutex::new(conn);

    let mut all_diagnostics = Vec::new();
    let mut cache_hits = 0usize;
    let mut cache_misses = 0usize;

    for file in files {
        let source = match std::fs::read_to_string(&file.path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Skip tiny files — not enough signal for LLM analysis.
        if source.lines().count() < 5 {
            continue;
        }

        let key = cache::cache_key("unified-llm", PROMPT_VERSION, &source);

        // Cache lookup.
        let cached = {
            let guard = conn.lock().unwrap_or_else(|e| e.into_inner());
            cache::lookup(&guard, &key)
        };

        if let Some(json) = cached {
            cache_hits += 1;
            if let Ok(diags) = serde_json::from_str::<Vec<Diagnostic>>(&json) {
                all_diagnostics.extend(diags);
            }
            continue;
        }

        cache_misses += 1;

        // Cache miss → single unified LLM call for ALL rules.
        match unified_prompt::evaluate_file(&source, &file.path, &config.model) {
            Ok(diags) => {
                // Store in cache.
                if let Ok(json) = serde_json::to_string(&diags) {
                    let guard = conn.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = cache::store(&guard, &key, &json);
                }
                all_diagnostics.extend(diags);
            }
            Err(e) => {
                eprintln!(
                    "comply: LLM failed on {}: {e:#}",
                    file.path.display()
                );
            }
        }
    }

    eprintln!(
        "comply: LLM pass — {cache_hits} cache hits, {cache_misses} misses ({} files skipped)",
        files.len() - cache_hits - cache_misses,
    );

    Ok(all_diagnostics)
}
