//! comply — your code will comply.
//!
//! Enforces coding-standards rules via syntactic analysis. Dispatches to oxlint
//! for TS/JS linting, applies custom tree-sitter rules in-process, and unifies
//! all output into ESLint-like format with remediation messages.
//!
//! Pipeline overview:
//! 1. Parse CLI args → ScanMode (which files to lint).
//! 2. Discover files via filesystem walk or git diff.
//! 3. For TS/JS files: invoke oxlint subprocess (if installed) AND apply
//!    custom tree-sitter rules. The two passes are complementary —
//!    oxlint catches type/style issues, custom rules catch architecture issues.
//! 4. For Rust files: apply custom rules only (clippy integration is v2).
//! 5. Apply comply-ignore suppressions across every discovered file.
//! 6. Format diagnostics, print, exit 0/1/2.

mod cli;
mod diagnostic;
mod engine;
mod files;
mod ignore_comments;
mod output;
mod oxlint;
mod rules;

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use cli::Cli;
use diagnostic::Diagnostic;
use files::{Language, SourceFile};

/// Embedded oxlint config — written to a per-invocation temp file at runtime
/// so concurrent invocations don't race on a shared `/tmp/comply/` path.
const OXLINTRC: &str = include_str!("oxlintrc.json");

fn main() -> ExitCode {
    match lint_project() {
        Ok(true) => ExitCode::from(1),  // violations found
        Ok(false) => ExitCode::from(0), // clean
        Err(e) => {
            eprintln!(
                "comply: crashed unexpectedly: {e:#}\n\
                 Re-run with RUST_BACKTRACE=1 and report at https://github.com/anthropics/comply/issues"
            );
            ExitCode::from(2)
        }
    }
}

/// Top-level orchestrator. Returns `true` if any violation was reported.
fn lint_project() -> Result<bool> {
    let cli = Cli::parse();
    let mode = cli.scan_mode();
    let discovered = files::discover(&mode)?;

    if discovered.is_empty() {
        println!("comply: no files to lint");
        return Ok(false);
    }

    let diagnostics = collect_all_diagnostics(&discovered)?;
    let after_suppressions = apply_ignore_suppressions(diagnostics, &discovered)?;

    report_diagnostics(&after_suppressions);
    Ok(!after_suppressions.is_empty())
}

/// Apply every linter (oxlint + custom rules) and collect diagnostics.
fn collect_all_diagnostics(discovered: &[SourceFile]) -> Result<Vec<Diagnostic>> {
    let (ts_files, rs_files) = partition_by_language(discovered);
    let mut diagnostics = Vec::with_capacity(discovered.len() * 2);

    if !ts_files.is_empty() {
        diagnostics.extend(lint_typescript(&ts_files)?);
    }
    if !rs_files.is_empty() {
        // Custom rules only — clippy integration is v2.
        diagnostics.extend(engine::lint_files(&rs_files)?);
    }

    Ok(diagnostics)
}

/// Split discovered files into TS/JS and Rust slices for language-specific dispatch.
fn partition_by_language(
    discovered: &[SourceFile],
) -> (Vec<&SourceFile>, Vec<&SourceFile>) {
    let ts_files = discovered
        .iter()
        .filter(|f| f.language == Language::TypeScript)
        .collect();
    let rs_files = discovered
        .iter()
        .filter(|f| f.language == Language::Rust)
        .collect();
    (ts_files, rs_files)
}

/// Lint TypeScript/JavaScript files via oxlint subprocess + custom rules.
fn lint_typescript(ts_files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    if oxlint::is_available() {
        let config = write_temp_oxlintrc()?;
        diagnostics.extend(oxlint::lint_files(ts_files, Some(config.path()))?);
        // `config` is held here so the temp file lives until oxlint finishes.
        drop(config);
    } else {
        eprintln!(
            "comply: oxlint not found — skipping oxlint rules. \
             Install with: npm install -g oxlint"
        );
    }

    diagnostics.extend(engine::lint_files(ts_files)?);
    Ok(diagnostics)
}

/// Print diagnostics in ESLint-like format and a summary line.
fn report_diagnostics(diagnostics: &[Diagnostic]) {
    if diagnostics.is_empty() {
        println!("comply: all clear");
        return;
    }
    let formatted = output::format_eslint(diagnostics);
    print!("{formatted}");
    eprintln!(
        "\ncomply: {} violation{} found",
        diagnostics.len(),
        if diagnostics.len() == 1 { "" } else { "s" }
    );
}

/// Hand-rolled temp file holder — writes the embedded oxlintrc to a unique
/// per-invocation file with `O_EXCL` semantics via `tempfile::NamedTempFile`.
struct TempConfig {
    inner: tempfile::NamedTempFile,
}

impl TempConfig {
    fn path(&self) -> &std::path::Path {
        self.inner.path()
    }
}

/// Write the embedded oxlintrc to a fresh temp file and return its handle.
///
/// Uses `tempfile::NamedTempFile` rather than a shared `/tmp/comply/` path:
/// - Concurrent comply invocations can't clobber each other (race-free).
/// - The unpredictable filename + `O_EXCL` mode prevents the classic
///   `/tmp` symlink attack where a malicious user pre-creates the path
///   as a symlink to a victim-writable file.
/// - The handle deletes the file on drop, so we don't litter /tmp.
fn write_temp_oxlintrc() -> Result<TempConfig> {
    let mut tmp = tempfile::Builder::new()
        .prefix("comply-")
        .suffix(".json")
        .tempfile()
        .context("failed to create temp oxlint config")?;
    tmp.write_all(OXLINTRC.as_bytes())
        .context("failed to write oxlint config to temp file")?;
    tmp.flush().context("failed to flush temp oxlint config")?;
    Ok(TempConfig { inner: tmp })
}

/// Apply comply-ignore suppressions to diagnostics.
///
/// Iterates over every discovered file (not just files with diagnostics) so
/// that malformed `comply-ignore` comments in clean files are still flagged.
fn apply_ignore_suppressions(
    diagnostics: Vec<Diagnostic>,
    discovered: &[SourceFile],
) -> Result<Vec<Diagnostic>> {
    let mut by_file = group_by_path(diagnostics);
    let mut result = Vec::with_capacity(by_file.len());

    for file in discovered {
        let file_diags = by_file.remove(&file.path).unwrap_or_default();
        let with_suppressions = suppress_for_file(file_diags, &file.path);
        result.extend(with_suppressions);
    }

    // Diagnostics for files NOT in the discovered list (e.g. paths normalized
    // differently by oxlint) — keep them as-is, no suppression.
    for (_, file_diags) in by_file {
        result.extend(file_diags);
    }

    Ok(result)
}

/// Group a flat diagnostic list by source path.
fn group_by_path(diagnostics: Vec<Diagnostic>) -> HashMap<PathBuf, Vec<Diagnostic>> {
    let mut by_file: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
    for d in diagnostics {
        by_file.entry(d.path.clone()).or_default().push(d);
    }
    by_file
}

/// Read source for one file and apply comply-ignore filtering.
/// Returns the original diagnostics unchanged if the file can't be read.
fn suppress_for_file(file_diags: Vec<Diagnostic>, path: &std::path::Path) -> Vec<Diagnostic> {
    match std::fs::read_to_string(path) {
        Ok(source) => ignore_comments::apply_suppressions(file_diags, path, &source),
        Err(_) => file_diags, // Source unreadable — bypass suppression rather than crash.
    }
}
