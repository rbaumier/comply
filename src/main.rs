//! comply — your code will comply.
//!
//! Enforces coding-standards rules via syntactic analysis. Dispatches to oxlint
//! for TS/JS linting, runs custom tree-sitter rules in-process, and unifies
//! all output into ESLint-like format with remediation messages.

mod cli;
mod diagnostic;
mod engine;
mod files;
mod ignore_comments;
mod output;
mod oxlint;
mod rules;

use std::fs;
use std::io::Write;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use diagnostic::Diagnostic;
use files::Language;

/// Embedded oxlint config — written to a temp file at runtime.
const OXLINTRC: &str = include_str!("oxlintrc.json");

fn main() -> ExitCode {
    match run() {
        Ok(has_violations) => {
            if has_violations {
                ExitCode::from(1)
            } else {
                ExitCode::from(0)
            }
        }
        Err(e) => {
            eprintln!("comply: internal error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    let mode = cli.scan_mode();
    let discovered = files::discover(&mode)?;

    if discovered.is_empty() {
        println!("comply: no files to lint");
        return Ok(false);
    }

    let mut all_diagnostics: Vec<Diagnostic> = Vec::new();

    // Partition files by language.
    let ts_files: Vec<_> = discovered
        .iter()
        .filter(|f| f.language == Language::TypeScript)
        .collect();
    let _rs_files: Vec<_> = discovered
        .iter()
        .filter(|f| f.language == Language::Rust)
        .collect();

    // --- TS/JS pipeline ---
    if !ts_files.is_empty() {
        // oxlint subprocess — skip silently if not installed (non-blocking).
        if oxlint::is_available() {
            let config_path = write_temp_oxlintrc()?;
            let oxlint_diags = oxlint::run(&ts_files, Some(config_path.as_path()))?;
            all_diagnostics.extend(oxlint_diags);
        } else {
            eprintln!(
                "comply: oxlint not found — skipping oxlint rules. \
                 Install with: npm install -g oxlint"
            );
        }

        // Custom tree-sitter rules on TS files.
        let custom_diags = engine::run_custom_rules(&ts_files)?;
        all_diagnostics.extend(custom_diags);
    }

    // --- Rust pipeline (v2 — custom rules only, no clippy yet) ---
    // Custom rules that apply to Rust (e.g. max-file-lines) already run
    // via engine::run_custom_rules since they declare Language::Rust.
    if !_rs_files.is_empty() {
        let rs_custom_diags = engine::run_custom_rules(&_rs_files)?;
        all_diagnostics.extend(rs_custom_diags);
    }

    // --- Apply comply-ignore suppressions ---
    // Pass all discovered files so even clean files are scanned for malformed
    // comply-ignore comments.
    all_diagnostics = apply_ignore_suppressions(all_diagnostics, &discovered)?;

    // --- Output ---
    let has_violations = !all_diagnostics.is_empty();
    if has_violations {
        let formatted = output::format_eslint(&all_diagnostics);
        print!("{formatted}");
        eprintln!(
            "\ncomply: {} violation{} found",
            all_diagnostics.len(),
            if all_diagnostics.len() == 1 { "" } else { "s" }
        );
    } else {
        println!("comply: all clear");
    }

    Ok(has_violations)
}

/// Write the embedded oxlintrc.json to a temp file. Returns the path.
fn write_temp_oxlintrc() -> Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join("comply");
    fs::create_dir_all(&dir)?;
    let path = dir.join("oxlintrc.json");
    let mut file = fs::File::create(&path)?;
    file.write_all(OXLINTRC.as_bytes())?;
    Ok(path)
}

/// Apply comply-ignore suppressions to diagnostics.
///
/// Iterates over every discovered file (not just files with diagnostics) so
/// that malformed `comply-ignore` comments in clean files are still flagged.
fn apply_ignore_suppressions(
    diagnostics: Vec<Diagnostic>,
    discovered: &[files::SourceFile],
) -> Result<Vec<Diagnostic>> {
    use std::collections::HashMap;

    // Group diagnostics by file path.
    let mut by_file: HashMap<std::path::PathBuf, Vec<Diagnostic>> = HashMap::new();
    for d in diagnostics {
        by_file.entry(d.path.clone()).or_default().push(d);
    }

    let mut result = Vec::new();

    // Process every discovered file — even ones with no diagnostics — so we
    // catch malformed comply-ignore comments everywhere.
    for file in discovered {
        let file_diags = by_file.remove(&file.path).unwrap_or_default();

        if let Ok(source) = fs::read_to_string(&file.path) {
            result.extend(ignore_comments::apply_suppressions(
                file_diags, &file.path, &source,
            ));
        } else {
            result.extend(file_diags);
        }
    }

    // Diagnostics for files NOT in the discovered list (e.g. from oxlint
    // resolving paths differently) — keep them as-is without suppression.
    for (_, file_diags) in by_file {
        result.extend(file_diags);
    }

    Ok(result)
}
