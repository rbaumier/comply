//! jscpd subprocess — copy/paste detection across the codebase.
//!
//! **STATUS: DISABLED** as of the perf audit — benchmarks showed jscpd
//! was responsible for 92% of wall-clock on a 216-file run (~105ms/file)
//! because it respawns a Node.js runtime on every call. The module is
//! kept compiled so we have a reference implementation to port to a
//! native Rust rule. See TODO.md "jscpd native replacement" for the
//! plan. Once the native rule is in place, delete this file.
//!
//! Why clone detection lives in Comply: the coding-standards skill flags
//! duplication as a Rule of Three signal — three similar snippets are a
//! pattern that should be extracted, two are coincidence.
//!
//! Original behaviour (for reference):
//! 1. `is_available()` probed `jscpd --version`, cached in a `OnceLock`.
//! 2. `lint_files()` collected the unique parent directories of the input
//!    files (jscpd scans directories, not individual files), ran jscpd on
//!    each, and parsed the JSON report. The default min-tokens threshold
//!    was 50.
//! 3. Each `duplicate` became one Comply diagnostic on the FIRST file of
//!    the pair, pointing to the start of the duplicated block.

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::runner_helpers;

pub const RULE_ID: &str = "no-clones";

pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| runner_helpers::probe_binary("jscpd", &["--version"]))
}

#[must_use = "diagnostics from jscpd must be reported"]
pub fn lint_files(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let mut diagnostics = Vec::new();
    for root in collect_scan_roots(files) {
        diagnostics.extend(scan_root(&root)?);
    }
    Ok(diagnostics)
}

/// jscpd scans directories rather than ancestor manifests, so it can't
/// use `runner_helpers::collect_unique_roots` (which expects a marker
/// filename like `package.json`). We collapse the input file list to its
/// set of canonicalized parent directories — `src/a.ts` and `src/b.ts`
/// both contribute `src/` once.
fn collect_scan_roots(files: &[&SourceFile]) -> HashSet<PathBuf> {
    files
        .iter()
        .filter_map(|f| f.path.parent().and_then(|p| p.canonicalize().ok()))
        .collect()
}

fn scan_root(root: &std::path::Path) -> Result<Vec<Diagnostic>> {
    // Per-invocation report dir so concurrent scans can't trample each
    // other. We mix the pid AND a per-call counter to avoid collisions
    // when one comply run scans multiple roots.
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let report_dir = std::env::temp_dir().join(format!(
        "comply-jscpd-{pid}-{id}",
        pid = std::process::id()
    ));
    let _ = std::fs::create_dir_all(&report_dir);
    // jscpd's default `--min-tokens=50` is calibrated for JavaScript and
    // produces a torrent of false positives on Rust, where ~10 lines of
    // trivial code (imports, struct field lists, match arms) can easily
    // hit 50 tokens with no real duplication. We bumped to 150 — the
    // sweet spot empirically for a mixed Rust+TS codebase. At that level
    // jscpd stops flagging the irreducible `pub struct Check; impl
    // AstCheck for Check { fn check { let source ...; walk_tree(...) } }`
    // preamble that every tree-sitter rule needs (one-time syntactic
    // overhead, not real duplication) but still catches the genuine
    // "I copied this 30-line helper into a sibling module" signal.
    let output = Command::new("jscpd")
        .args(["--reporters", "json", "--silent", "--min-tokens", "150", "--output"])
        .arg(&report_dir)
        .arg(root)
        .output()
        .with_context(|| format!("failed to invoke `jscpd` on {}", root.display()))?;
    if !output.status.success() && output.stdout.is_empty() {
        // jscpd exits non-zero on internal errors. Skip this scan rather
        // than failing the whole comply run.
        let _ = std::fs::remove_dir_all(&report_dir);
        return Ok(vec![]);
    }
    let report_path = report_dir.join("jscpd-report.json");
    // jscpd doesn't write a report file when it scans an "empty" tree
    // (no files matching its supported formats). That's a clean scan,
    // not an error — return zero diagnostics quietly.
    let Ok(bytes) = std::fs::read(&report_path) else {
        let _ = std::fs::remove_dir_all(&report_dir);
        return Ok(vec![]);
    };
    let _ = std::fs::remove_dir_all(&report_dir);
    let report: JscpdReport = serde_json::from_slice(&bytes)
        .with_context(|| "failed to parse jscpd JSON report")?;
    Ok(convert_duplicates(report.duplicates))
}

fn convert_duplicates(duplicates: Vec<Duplicate>) -> Vec<Diagnostic> {
    duplicates
        .into_iter()
        .map(|d| Diagnostic {
            path: PathBuf::from(&d.first_file.name),
            line: d.first_file.start_loc.line,
            column: d.first_file.start_loc.column,
            rule_id: RULE_ID.into(),
            message: format!(
                "Duplicated block ({lines} lines) — also appears in `{other}` at line {other_line}. \
                 Three similar snippets are a Rule of Three signal: extract a shared helper. \
                 Two clones can wait, but if a third appears, refactor.",
                lines = d.lines,
                other = d.second_file.name,
                other_line = d.second_file.start_loc.line
            ),
            severity: Severity::Warning,
            span: None,
        })
        .collect()
}

//
// jscpd's `firstFile.start` field is a token offset (a number), not a
// line/column object. The actual source position lives under
// `firstFile.startLoc` as `{ line, column, position }`. We deserialize
// `startLoc` and ignore the token-offset `start` entirely.

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
struct JscpdReport {
    #[serde(default)]
    duplicates: Vec<Duplicate>,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
struct Duplicate {
    lines: usize,
    #[serde(rename = "firstFile")]
    first_file: FilePosition,
    #[serde(rename = "secondFile")]
    second_file: FilePosition,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
struct FilePosition {
    name: String,
    #[serde(rename = "startLoc")]
    start_loc: LineCol,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
struct LineCol {
    line: usize,
    column: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_duplicate_report() {
        let raw = br#"{"duplicates":[{"format":"typescript","lines":12,"tokens":80,"firstFile":{"name":"a.ts","start":1,"end":12,"startLoc":{"line":3,"column":1,"position":5},"endLoc":{"line":15,"column":2,"position":200}},"secondFile":{"name":"b.ts","start":1,"end":12,"startLoc":{"line":7,"column":1,"position":5},"endLoc":{"line":19,"column":2,"position":200}}}]}"#;
        let report: JscpdReport = serde_json::from_slice(raw).unwrap();
        assert_eq!(report.duplicates.len(), 1);
        assert_eq!(report.duplicates[0].first_file.name, "a.ts");
        assert_eq!(report.duplicates[0].lines, 12);
        assert_eq!(report.duplicates[0].first_file.start_loc.line, 3);
    }

    #[test]
    fn empty_report() {
        let raw = br#"{"duplicates":[]}"#;
        let report: JscpdReport = serde_json::from_slice(raw).unwrap();
        assert!(report.duplicates.is_empty());
    }
}
