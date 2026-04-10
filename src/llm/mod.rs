//! LLM backend — semantic rule evaluation via Bun worker.
//!
//! Activated by `comply --with-llm`. Builds prompts in Rust (snippet
//! extraction, unified prompt), then sends all jobs to a single Bun
//! subprocess (`tools/llm-worker.ts`) that runs them in parallel via
//! the Vercel AI SDK + claude-code provider. Results stream back as
//! NDJSON. Cached in SQLite per-project.

pub mod cache;
pub mod claude_cli;
pub mod extract;
pub mod pool;
pub mod unified_prompt;

use anyhow::{Context, Result};

use crate::diagnostic::Diagnostic;

/// Cache version — bump when the unified prompt changes materially.
const PROMPT_VERSION: u32 = 8;

/// Configuration for an LLM lint pass.
#[derive(Debug)]
pub struct LlmConfig {
    pub model: String,
    pub concurrency: usize,
    pub project_root: std::path::PathBuf,
}

/// A file ready for LLM evaluation — source already read and cache
/// checked. Sent to the Bun worker.
struct LlmJob {
    source: String,
    path: std::path::PathBuf,
    cache_key: String,
}

/// JSON sent to the Bun worker on stdin.
#[derive(serde::Serialize)]
struct WorkerJob {
    id: String,
    prompt: String,
    model: String,
}

/// One NDJSON line read from the Bun worker stdout.
#[derive(serde::Deserialize)]
struct WorkerResult {
    id: String,
    result: Option<String>,
    error: Option<String>,
}

/// Run the unified LLM prompt on every file via the Bun worker.
pub fn lint_files(
    files: &[&crate::files::SourceFile],
    config: &LlmConfig,
) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }

    let conn = cache::open(&config.project_root)
        .context("failed to open LLM cache")?;

    // Phase 1: read files, check cache, build prompts for misses.
    let mut all_diagnostics = Vec::new();
    let mut cache_hits = 0usize;
    let mut jobs: Vec<LlmJob> = Vec::new();

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

    // Phase 2: one worker job per file — full source, all 9 rules.
    let worker_jobs: Vec<WorkerJob> = jobs
        .iter()
        .map(|job| WorkerJob {
            id: job.path.display().to_string(),
            prompt: unified_prompt::build_prompt(&job.source),
            model: config.model.clone(),
        })
        .collect();

    eprintln!("comply: LLM — {} files to evaluate", worker_jobs.len());

    // Phase 3: invoke Bun worker.
    let worker_results = invoke_worker(&worker_jobs)?;

    // Phase 4: parse results, update cache.
    for job in &jobs {
        let file_id = job.path.display().to_string();

        let wr = match worker_results.iter().find(|r| r.id == file_id) {
            Some(r) => r,
            None => continue,
        };

        if let Some(ref err) = wr.error {
            eprintln!("comply: LLM failed on {file_id}: {err}");
            continue;
        }

        let diags = match wr.result.as_deref() {
            Some(json) => match unified_prompt::parse_response(json, &job.path) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("comply: LLM parse failed on {file_id}: {e:#}");
                    continue;
                }
            },
            None => continue,
        };

        if let Ok(json) = serde_json::to_string(&diags) {
            let _ = cache::store(&conn, &job.cache_key, &json);
        }
        all_diagnostics.extend(diags);
    }

    eprintln!(
        "comply: LLM pass — {cache_hits} cache hits, {cache_misses} misses ({skipped} files skipped)",
    );

    Ok(all_diagnostics)
}

/// Spawn the Bun worker, send jobs on stdin, read NDJSON results.
fn invoke_worker(jobs: &[WorkerJob]) -> Result<Vec<WorkerResult>> {
    use std::io::{BufRead, Write};
    use std::process::{Command, Stdio};

    // Locate the worker script relative to the comply binary.
    let worker_path = worker_script_path()?;

    let mut child = Command::new("bun")
        .arg("run")
        .arg(&worker_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to spawn bun worker — is bun installed?")?;

    // Send all jobs as JSON on stdin.
    {
        let mut stdin = child.stdin.take().context("failed to open worker stdin")?;
        let payload = serde_json::to_string(jobs).context("failed to serialize jobs")?;
        stdin.write_all(payload.as_bytes()).context("failed to write to worker stdin")?;
        stdin.flush()?;
    }

    // Read NDJSON results from stdout.
    let stdout = child.stdout.take().context("failed to open worker stdout")?;
    let reader = std::io::BufReader::new(stdout);
    let mut results = Vec::with_capacity(jobs.len());

    for line in reader.lines() {
        let line = line.context("failed to read worker output line")?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<WorkerResult>(&line) {
            Ok(wr) => results.push(wr),
            Err(e) => {
                eprintln!("comply: failed to parse worker output: {e}");
            }
        }
    }

    let status = child.wait().context("worker process failed")?;
    if !status.success() {
        eprintln!("comply: bun worker exited with {status}");
    }

    Ok(results)
}

/// Find `tools/llm-worker.ts` relative to the comply binary or cwd.
fn worker_script_path() -> Result<std::path::PathBuf> {
    // Try relative to the binary.
    if let Ok(exe) = std::env::current_exe() {
        let from_exe = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.join("tools/llm-worker.ts"));
        if let Some(ref path) = from_exe {
            if path.exists() {
                return Ok(path.clone());
            }
        }
    }

    // Try relative to cwd.
    let from_cwd = std::path::PathBuf::from("tools/llm-worker.ts");
    if from_cwd.exists() {
        return Ok(from_cwd);
    }

    // Hardcoded fallback for development.
    let dev = std::path::PathBuf::from(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tools/llm-worker.ts"),
    );
    if dev.exists() {
        return Ok(dev);
    }

    anyhow::bail!(
        "cannot find tools/llm-worker.ts — run comply from the project root"
    )
}
