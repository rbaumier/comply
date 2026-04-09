//! cargo-shear subprocess — flag unused dependencies in `Cargo.toml`.
//!
//! Why this lives in Comply: dead deps are a security and compile-time
//! liability. Each unused crate widens the supply chain, slows builds, and
//! adds version-bump churn. `cargo shear` (https://crates.io/crates/cargo-shear)
//! detects them by walking the source tree and comparing imports against
//! the `[dependencies]` table.
//!
//! How it works:
//! 1. `is_available()` probes `cargo shear --version`. Cached in a
//!    `OnceLock`.
//! 2. `lint_files()` finds the workspace root for any `.rs` file in the
//!    input set (the nearest `Cargo.toml` ancestor) and runs:
//!
//!        cargo shear --format=json <workspace>
//!
//! 3. The JSON output is parsed; each "finding" of `code = shear/unused_dependency`
//!    becomes one comply diagnostic on the offending `Cargo.toml`. The
//!    byte `offset` is converted to a 1-based line/column by reading the
//!    manifest and counting newlines.
//!
//! Workspaces are de-duplicated so we don't shell out twice for the same
//! manifest when many `.rs` files share a parent crate.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;

/// Stable comply rule id surfaced in diagnostics for unused-dependency
/// findings. Mirrors the convention used by clippy/oxlint runners.
pub const RULE_ID: &str = "rust-unused-dep";

/// Cached availability probe for `cargo shear`.
pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("cargo")
            .args(["shear", "--version"])
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// Run `cargo shear` on every workspace touched by `files` and return the
/// remapped diagnostics. Files outside any workspace are skipped silently —
/// shear can only operate on a real Cargo project root.
#[must_use = "diagnostics from cargo-shear must be reported"]
pub fn lint_files(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let workspaces = collect_workspaces(files);
    let mut diagnostics = Vec::new();
    for workspace in workspaces {
        diagnostics.extend(lint_workspace(&workspace)?);
    }
    Ok(diagnostics)
}

/// Walk every input file and collect the unique set of workspace roots.
/// A workspace root is the nearest ancestor directory containing a
/// `Cargo.toml`. Files outside any workspace are dropped.
fn collect_workspaces(files: &[&SourceFile]) -> HashSet<PathBuf> {
    let mut roots = HashSet::new();
    for f in files {
        if let Some(root) = find_cargo_root(&f.path) {
            roots.insert(root);
        }
    }
    roots
}

/// Walk parents until we find a `Cargo.toml`. Returns the directory
/// containing it, not the manifest itself. We canonicalize first so that
/// callers passing relative paths (`src/main.rs`) still get an absolute
/// workspace root — without canonicalization, walking parents stops at
/// the first relative segment and returns an empty path.
fn find_cargo_root(path: &Path) -> Option<PathBuf> {
    let canonical = path.canonicalize().ok()?;
    let mut current = canonical.parent();
    while let Some(dir) = current {
        if dir.join("Cargo.toml").is_file() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// Run `cargo shear --format=json` from inside `workspace` and parse the
/// result. We `current_dir(workspace)` instead of passing it as a positional
/// argument because shear honors `CARGO_MANIFEST_DIR` and the cwd more
/// reliably than its `PATH` argument when invoked from a child process.
fn lint_workspace(workspace: &Path) -> Result<Vec<Diagnostic>> {
    let output = Command::new("cargo")
        .args(["shear", "--format=json"])
        .current_dir(workspace)
        .output()
        .with_context(|| format!("failed to invoke `cargo shear` in {}", workspace.display()))?;
    // Empty stdout = nothing to parse. Either shear truly found nothing
    // or it errored before producing output; either way, return cleanly
    // rather than crashing on empty JSON.
    if output.stdout.is_empty() {
        return Ok(vec![]);
    }
    let report: ShearReport = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "failed to parse cargo-shear JSON output from {}",
            workspace.display()
        )
    })?;
    convert_findings(report.findings, workspace)
}

/// Convert each shear finding into a comply Diagnostic. We only emit
/// diagnostics for `shear/unused_dependency` findings — shear also surfaces
/// `shear/unlinked_files` (orphan source files) but those are out of scope
/// for a "no unused deps" rule and would muddy the signal. Each unused-dep
/// finding includes a `file` and a byte `offset`; we translate the offset
/// to a 1-based line/column for editor compatibility.
fn convert_findings(findings: Vec<Finding>, workspace: &Path) -> Result<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    for finding in findings {
        if finding.code != "shear/unused_dependency" {
            continue;
        }
        let Some(file) = finding.file else { continue };
        let manifest_path = workspace.join(file);
        let offset = finding.location.map(|l| l.offset).unwrap_or(0);
        let (line, column) =
            byte_offset_to_line_col(&manifest_path, offset).unwrap_or((1, 1));
        diagnostics.push(Diagnostic {
            path: manifest_path,
            line,
            column,
            rule_id: RULE_ID.into(),
            message: format!(
                "{} — every unused dep widens the supply chain, slows builds, \
                 and adds version-bump churn. Remove it from `[dependencies]` \
                 (or run `cargo shear --fix`).",
                finding.message
            ),
            severity: match finding.severity.as_str() {
                "error" => Severity::Error,
                _ => Severity::Warning,
            },
        });
    }
    Ok(diagnostics)
}

/// Read `path` and convert a byte `offset` into a 1-based (line, column).
/// Falls back to (1, 1) if the file can't be read — better to surface the
/// finding at the top of the manifest than to drop it entirely.
fn byte_offset_to_line_col(path: &Path, offset: usize) -> Option<(usize, usize)> {
    let content = fs::read_to_string(path).ok()?;
    let bytes = content.as_bytes();
    let clamped = offset.min(bytes.len());
    let mut line = 1usize;
    let mut col = 1usize;
    for &b in &bytes[..clamped] {
        if b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    Some((line, col))
}

// ---------- JSON schema ----------

#[derive(Debug, Deserialize)]
struct ShearReport {
    #[serde(default)]
    findings: Vec<Finding>,
}

#[derive(Debug, Deserialize)]
struct Finding {
    code: String,
    severity: String,
    message: String,
    /// Optional — `shear/unlinked_files` findings have no file field.
    /// We skip those upstream so the absence is fine.
    #[serde(default)]
    file: Option<String>,
    /// Optional for the same reason.
    #[serde(default)]
    location: Option<Location>,
}

#[derive(Debug, Deserialize)]
struct Location {
    offset: usize,
    #[allow(dead_code)]
    length: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_finding_json() {
        let raw = br#"{"summary":{"errors":1,"warnings":0,"fixed":0},"findings":[{"code":"shear/unused_dependency","severity":"error","message":"unused dependency `serde`","file":"Cargo.toml","location":{"offset":75,"length":5},"help":"remove this dependency","fixable":true}]}"#;
        let report: ShearReport = serde_json::from_slice(raw).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].message, "unused dependency `serde`");
    }

    #[test]
    fn empty_findings_array() {
        let raw = br#"{"summary":{"errors":0,"warnings":0,"fixed":0},"findings":[]}"#;
        let report: ShearReport = serde_json::from_slice(raw).unwrap();
        assert!(report.findings.is_empty());
    }

    #[test]
    fn byte_offset_at_start() {
        let tmp = std::env::temp_dir().join("comply-shear-test.toml");
        fs::write(&tmp, "line1\nline2\nline3").unwrap();
        assert_eq!(byte_offset_to_line_col(&tmp, 0), Some((1, 1)));
        assert_eq!(byte_offset_to_line_col(&tmp, 6), Some((2, 1)));
        assert_eq!(byte_offset_to_line_col(&tmp, 12), Some((3, 1)));
        let _ = fs::remove_file(&tmp);
    }
}
