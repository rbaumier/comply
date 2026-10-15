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
    // Per-package on purpose: `cargo modules orphans` walks a single crate's
    // module tree and has no `--workspace` mode, so we group by the nearest
    // manifest (the member) rather than the workspace root the way the clippy
    // and shear runners do.
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
    // exist. A run that fails before walking anything prints on stderr
    // instead, so that is where its reason is read from.
    let said = if output.stdout.is_empty() { &output.stderr } else { &output.stdout };
    let text = String::from_utf8_lossy(said).into_owned();
    read_orphans(&text, output.status.success(), workspace)
}

/// How many lines of the tool's own words a refusal quotes.
const QUOTED_LINES: usize = 5;

/// The orphans `said` names, or the refusal of a scan that read nothing.
///
/// `cargo modules orphans` exits non-zero in two opposite cases: when it found
/// orphans, and when it could not walk the module tree at all. The exit code was
/// ignored, so the second case returned an empty list that read as "no orphan in
/// this crate" — a verdict on a tree nobody walked. A failure that names an orphan
/// is a finding; a failure that names none is a refusal.
fn read_orphans(said: &str, succeeded: bool, workspace: &Path) -> Result<Vec<Diagnostic>> {
    let orphans = parse_orphans(said, workspace);
    if orphans.is_empty() && !succeeded {
        let detail = strip_ansi(said).lines().take(QUOTED_LINES).collect::<Vec<_>>().join("\n");
        return Err(crate::toolchain::ToolchainError::Unusable {
            what: format!(
                "cargo modules orphans failed in {} without naming one orphan; no module tree \
                 was walked. Repair the crate, then re-run comply.\n{}",
                workspace.display(),
                if detail.trim().is_empty() { "(no output)" } else { detail.trim() }
            ),
        }
        .into());
    }
    Ok(orphans)
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
            severity: Severity::Error,
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

    /// `cargo modules orphans` exits 1 both when it found orphans and when
    /// it could not scan at all. The exit code was ignored, so a crate it
    /// never walked yielded an empty list that read as "no orphan here".
    #[test]
    fn a_failed_scan_that_found_nothing_is_refused() {
        let ws = Path::new("/proj");
        let found = "warning: orphaned module 'rust' at src/rules/foo/rust.rs";
        assert!(
            read_orphans(found, false, ws).is_ok_and(|d| d.len() == 1),
            "exit 1 with an orphan is how a finding is reported"
        );
        assert!(
            read_orphans("No orphans found.", true, ws).is_ok_and(|d| d.is_empty()),
            "exit 0 with nothing parsed is a genuinely clean crate"
        );
        assert!(
            read_orphans("error: failed to parse manifest", false, ws).is_err(),
            "exit 1 with nothing parsed is a crate that was never walked"
        );
    }

    #[test]
    fn ignores_non_orphan_lines() {
        let text = "Compiling foo\nFinished dev profile\n";
        assert!(parse_orphans(text, Path::new("/proj")).is_empty());
    }
}
