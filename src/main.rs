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
mod clippy;
mod diagnostic;
mod engine;
mod explain;
mod files;
mod ignore_comments;
mod list;
mod output;
mod oxlint;
mod oxlint_config;
mod rules;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use diagnostic::Diagnostic;
use files::{Language, SourceFile};

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::from(1),  // violations found
        Ok(false) => ExitCode::from(0), // clean
        Err(e) => {
            eprintln!(
                "comply: crashed unexpectedly: {e:#}\n\
                 Re-run with RUST_BACKTRACE=1 and report at https://github.com/rbaumier/comply/issues"
            );
            ExitCode::from(2)
        }
    }
}

/// Dispatch on the top-level subcommand. Default = lint.
fn run() -> Result<bool> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Explain { ref rule_id }) => {
            explain::run(rule_id)?;
            Ok(false)
        }
        Some(Command::List { json }) => {
            list::run(json)?;
            Ok(false)
        }
        None => lint_project(&cli),
    }
}

/// Top-level lint orchestrator. Returns `true` if any violation was reported.
fn lint_project(cli: &Cli) -> Result<bool> {
    let mode = cli.scan_mode();
    let discovered = files::discover(&mode)?;

    if discovered.is_empty() {
        if !cli.json {
            println!("comply: no files to lint");
        } else {
            println!("[]");
        }
        return Ok(false);
    }

    let diagnostics = collect_all_diagnostics(&discovered)?;
    let after_suppressions = ignore_comments::apply_to_all(diagnostics, &discovered);

    if cli.json {
        report_diagnostics_json(&after_suppressions)?;
    } else {
        report_diagnostics(&after_suppressions);
    }
    Ok(!after_suppressions.is_empty())
}

/// Apply every linter (oxlint + custom rules) and collect diagnostics.
fn collect_all_diagnostics(discovered: &[SourceFile]) -> Result<Vec<Diagnostic>> {
    let (ts_files, rs_files) = partition_by_language(discovered);
    let mut diagnostics = Vec::with_capacity(discovered.len());

    if !ts_files.is_empty() {
        diagnostics.extend(lint_typescript(&ts_files)?);
    }
    if !rs_files.is_empty() {
        diagnostics.extend(lint_rust(&rs_files)?);
    }

    Ok(diagnostics)
}

/// Lint Rust files via clippy subprocess + custom tree-sitter rules.
/// The two passes are complementary: clippy catches type-aware lints
/// and the standard library footguns; custom rules catch the architecture
/// and naming concerns that clippy doesn't model.
fn lint_rust(rs_files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    if clippy::is_available() {
        diagnostics.extend(clippy::lint_files(rs_files)?);
    } else {
        eprintln!(
            "comply: cargo clippy not found — skipping clippy-backed rules. \
             Install with: rustup component add clippy"
        );
    }

    diagnostics.extend(engine::lint_files(rs_files)?);
    Ok(diagnostics)
}

/// Split discovered files into TS-family and Rust slices for dispatch.
/// Without this split we'd hand .rs files to oxlint, which would error.
fn partition_by_language(discovered: &[SourceFile]) -> (Vec<&SourceFile>, Vec<&SourceFile>) {
    let ts_files = discovered
        .iter()
        .filter(|f| f.language.is_typescript_family())
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
        // oxlint::lint_files now owns its own temp-config lifecycle —
        // it collects Backend::Oxlint bindings from the rule registry,
        // generates the oxlintrc, and holds the NamedTempFile until the
        // subprocess finishes reading it.
        diagnostics.extend(oxlint::lint_files(ts_files)?);
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

/// Print diagnostics as a JSON array — nothing else on stdout so editors
/// and CI tools can pipe the output directly into a parser.
fn report_diagnostics_json(diagnostics: &[Diagnostic]) -> Result<()> {
    let json = output::format_json(diagnostics)?;
    println!("{json}");
    Ok(())
}
