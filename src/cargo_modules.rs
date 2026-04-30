//! cargo-modules subprocess — orphan Rust source files.
//!
//! Why this lives in Comply: a `.rs` file that no `mod` declaration loads
//! is dead code with worse properties than a regular orphan — rust-analyzer
//! still sees it, search tools index it, and grep returns matches that
//! point nowhere. cargo-modules
//! (https://github.com/regexident/cargo-modules) crawls the module tree
//! and lists every file the compiler can't reach.
//!
//! How it works:
//! 1. `is_available()` probes `cargo modules --version`. Cached.
//! 2. `lint_files()` finds the unique workspace roots (the nearest
//!    `Cargo.toml` ancestor) and runs:
//!
//!        cargo modules orphans --manifest-path <root>/Cargo.toml
//!
//!    cargo-modules outputs colored text (no JSON reporter as of v0.25),
//!    so we strip ANSI escapes and parse the `warning: orphaned module
//!    'X' at <path>` lines via a small regex.
//! 3. Each parsed orphan becomes one Comply diagnostic on the offending
//!    file path.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::runner_helpers;

pub const RULE_ID: &str = "rust-orphan-module";

pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| runner_helpers::probe_binary("cargo", &["modules", "--version"]))
}

#[must_use = "diagnostics from cargo-modules must be reported"]
pub fn lint_files(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let mut diagnostics = Vec::new();
    for workspace in runner_helpers::collect_unique_roots(files, "Cargo.toml") {
        diagnostics.extend(scan_workspace(&workspace)?);
    }
    Ok(diagnostics)
}

fn scan_workspace(workspace: &Path) -> Result<Vec<Diagnostic>> {
    let output = Command::new("cargo")
        .args(["modules", "orphans", "--manifest-path"])
        .arg(workspace.join("Cargo.toml"))
        .output()
        .with_context(|| {
            format!(
                "failed to invoke `cargo modules orphans` in {}",
                workspace.display()
            )
        })?;
    // cargo-modules writes its rustc-style warnings to STDOUT (not stderr,
    // contrary to most cargo subcommands) and exits non-zero when orphans
    // exist — we expect both, so we read stdout and ignore the exit code.
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(parse_orphans(&stdout, workspace))
}

/// Strip ANSI color escapes and pull `at <path>` snippets out of the
/// `warning: orphaned module ... at <path>` lines.
fn parse_orphans(text: &str, workspace: &Path) -> Vec<Diagnostic> {
    let stripped = strip_ansi(text);
    let mut diagnostics = Vec::new();
    for line in stripped.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("warning: orphaned module") else {
            continue;
        };
        // The line shape is: ` 'name' at <path>`. We pull both segments.
        let Some(at_pos) = rest.find(" at ") else {
            continue;
        };
        // cargo-modules formats the module name with backticks: `name`.
        // Strip both backticks AND apostrophes so the diagnostic message
        // doesn't render as `` `name` ``.
        let module_part = rest[..at_pos]
            .trim()
            .trim_matches(|c: char| c == '`' || c == '\'')
            .to_string();
        let path_part = rest[at_pos + 4..].trim().to_string();
        let absolute = workspace.join(&path_part);
        diagnostics.push(Diagnostic {
            path: absolute.into(),
            line: 1,
            column: 1,
            rule_id: RULE_ID.into(),
            message: format!(
                "Orphan module `{module_part}` — no `mod {module_part};` declaration \
                 loads this file. Either declare it from the parent module or delete \
                 the file. Orphan files are dead code that grep still finds."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
    diagnostics
}

/// Minimal ANSI escape stripper — drops `ESC [ ... letter` sequences.
/// We don't need a full terminal emulator; cargo-modules only emits SGR
/// codes (color/format) and these always end in a single ASCII letter.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.next() != Some('[') {
                continue;
            }
            // Skip until terminator (an ASCII letter).
            while let Some(&next) = chars.peek() {
                chars.next();
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi_escapes() {
        let input = "\x1b[1;31mhello\x1b[0m world";
        assert_eq!(strip_ansi(input), "hello world");
    }

    #[test]
    fn parses_orphan_line() {
        let text = "warning: orphaned module 'rust' at src/rules/foo/rust.rs\n  --> ...";
        let diagnostics = parse_orphans(text, Path::new("/proj"));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].path.ends_with("src/rules/foo/rust.rs"));
        assert!(diagnostics[0].message.contains("rust"));
    }

    #[test]
    fn ignores_non_orphan_lines() {
        let text = "Compiling foo\nFinished dev profile\n";
        assert!(parse_orphans(text, Path::new("/proj")).is_empty());
    }
}
