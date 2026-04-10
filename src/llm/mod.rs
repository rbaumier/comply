//! LLM backend — semantic rule evaluation via `claude` CLI.
//!
//! Activated by `comply --with-llm`. For each file, each registered LLM
//! rule builds a prompt, checks the SQLite cache, and on cache miss
//! spawns a `claude -p` subprocess to evaluate the rule. Results are
//! cached for future runs (hash-based, no TTL).
//!
//! The module is a thin orchestrator: the actual rule logic (prompt
//! construction + response parsing) lives in the individual rule modules
//! under `src/rules/llm_*`.

pub mod cache;
pub mod claude_cli;
pub mod pool;

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Mutex;

use crate::diagnostic::Diagnostic;

/// An LLM-powered rule that evaluates a code block via the claude CLI.
///
/// Each rule implements this trait. The orchestrator calls `check_block`
/// for every top-level code block in the file; the rule decides whether
/// the block is relevant (e.g., only comments, only functions > 20 lines)
/// and builds the prompt.
pub trait LlmRule: Send + Sync {
    /// Stable rule id (e.g., "llm-comment-quality").
    fn rule_id(&self) -> &'static str;

    /// Rule version — bump when the prompt changes to auto-invalidate cache.
    fn rule_version(&self) -> u32;

    /// Check a single code block. Returns diagnostics for this block,
    /// or an empty vec if the block is not relevant to this rule.
    ///
    /// `block` is the source text of one top-level item (function, struct,
    /// type, const, or comment block). `file_path` and `block_start_line`
    /// are used to construct diagnostics with correct locations.
    fn check_block(
        &self,
        block: &str,
        file_path: &Path,
        block_start_line: usize,
        model: &str,
    ) -> Result<Vec<Diagnostic>>;
}

/// Configuration for an LLM lint pass.
#[derive(Debug)]
pub struct LlmConfig {
    pub model: String,
    pub concurrency: usize,
    pub project_root: std::path::PathBuf,
}

/// Run all LLM rules on the given files. Returns diagnostics from all rules.
///
/// This is synchronous — it builds a small tokio runtime internally so
/// the caller (main.rs) doesn't need to be async. The runtime is used
/// solely for the Semaphore-based concurrency limiter.
pub fn lint_files(
    files: &[&crate::files::SourceFile],
    rules: &[Box<dyn LlmRule>],
    config: &LlmConfig,
) -> Result<Vec<Diagnostic>> {
    if files.is_empty() || rules.is_empty() {
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

        // For now, treat the entire file as one block.
        // Future: split into top-level AST nodes for granular caching.
        let block = &source;
        let block_start_line = 1;

        for rule in rules {
            let key = cache::cache_key(rule.rule_id(), rule.rule_version(), block);

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

            // Cache miss → invoke LLM.
            match rule.check_block(block, &file.path, block_start_line, &config.model) {
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
                        "comply: LLM rule {} failed on {}: {e:#}",
                        rule.rule_id(),
                        file.path.display()
                    );
                }
            }
        }
    }

    eprintln!(
        "comply: LLM pass — {cache_hits} cache hits, {cache_misses} misses"
    );

    Ok(all_diagnostics)
}
