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
const PROMPT_VERSION: u32 = 3;

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

    // Phase 2: build worker jobs (extract snippets + build prompt).
    let worker_jobs: Vec<WorkerJob> = jobs
        .iter()
        .flat_map(|job| {
            let snippets = extract::extract_snippets(&job.source);
            let snippet_lines: Vec<&str> = snippets.lines().collect();

            // Split large extractions into chunks at gap markers.
            let chunks = split_into_chunks(&snippet_lines, 400);
            let num_chunks = chunks.len();

            chunks
                .into_iter()
                .enumerate()
                .map(|(chunk_idx, chunk)| {
                    let id = if num_chunks == 1 {
                        job.path.display().to_string()
                    } else {
                        format!("{}#chunk{}", job.path.display(), chunk_idx)
                    };
                    WorkerJob {
                        id,
                        prompt: unified_prompt::build_prompt(&chunk),
                        model: config.model.clone(),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();

    eprintln!(
        "comply: LLM — {} files, {} chunks to evaluate",
        cache_misses,
        worker_jobs.len(),
    );

    // Phase 3: invoke Bun worker.
    let worker_results = invoke_worker(&worker_jobs)?;

    // Phase 4: parse results, update cache.
    // Group results by file path (strip #chunkN suffix).
    for job in &jobs {
        let file_id = job.path.display().to_string();
        let mut file_diags: Vec<Diagnostic> = Vec::new();

        for wr in &worker_results {
            let wr_file = wr.id.split("#chunk").next().unwrap_or(&wr.id);
            if wr_file != file_id {
                continue;
            }

            if let Some(ref err) = wr.error {
                eprintln!("comply: LLM failed on {}: {err}", wr.id);
                continue;
            }

            if let Some(ref json) = wr.result {
                match unified_prompt::parse_response(json, &job.path) {
                    Ok(diags) => file_diags.extend(diags),
                    Err(e) => {
                        eprintln!("comply: LLM parse failed on {}: {e:#}", wr.id);
                    }
                }
            }
        }

        // Cache the merged diagnostics for the whole file.
        if let Ok(json) = serde_json::to_string(&file_diags) {
            let _ = cache::store(&conn, &job.cache_key, &json);
        }
        all_diagnostics.extend(file_diags);
    }

    eprintln!(
        "comply: LLM pass — {cache_hits} cache hits, {cache_misses} misses ({skipped} files skipped)",
    );

    Ok(all_diagnostics)
}

/// Split snippet lines into chunks of at most `max_lines`, cutting at
/// gap markers ("... (lines N-M omitted)") to avoid splitting blocks.
fn split_into_chunks(lines: &[&str], max_lines: usize) -> Vec<String> {
    if lines.len() <= max_lines {
        return vec![lines.join("\n")];
    }

    let mut chunks = Vec::new();
    let mut chunk = String::new();
    let mut chunk_len = 0;

    for line in lines {
        let is_gap = line.starts_with("... (lines ");

        if chunk_len >= max_lines && is_gap && !chunk.is_empty() {
            chunks.push(std::mem::take(&mut chunk));
            chunk_len = 0;
        }

        chunk.push_str(line);
        chunk.push('\n');
        chunk_len += 1;
    }

    if !chunk.is_empty() {
        chunks.push(chunk);
    }

    chunks
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
