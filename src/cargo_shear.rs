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

use rustc_hash::FxHashSet;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::runner_helpers;

/// Stable comply rule id surfaced in diagnostics for unused-dependency
/// findings. Mirrors the convention used by clippy/oxlint runners.
pub const RULE_ID: &str = "rust-unused-dep";

/// Cached availability probe for `cargo shear`.
pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| runner_helpers::probe_binary("cargo", &["shear", "--version"]))
}

/// Run `cargo shear` on every workspace touched by `files` and return the
/// remapped diagnostics. Files outside any workspace are skipped silently —
/// shear can only operate on a real Cargo project root.
#[must_use = "diagnostics from cargo-shear must be reported"]
pub fn lint_files(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let mut diagnostics = Vec::new();
    let mut roots = FxHashSet::default();
    for f in files {
        if let Some(root) = runner_helpers::find_cargo_workspace_root(&f.path) {
            roots.insert(root);
        }
    }
    for workspace in roots {
        diagnostics.extend(lint_workspace(&workspace)?);
    }
    Ok(diagnostics)
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
    let report = read_report(&output.stdout, &output.stderr, workspace)?;
    convert_findings(report.findings, workspace)
}

/// How many lines of the tool's own words a refusal quotes.
const QUOTED_LINES: usize = 5;

/// The shear report, or the refusal of a run comply can draw nothing from.
///
/// Two silences used to read as a clean workspace. `cargo shear --format=json`
/// prints `{"findings":[]}` when it finds nothing, so an empty stdout is never a
/// result: it is a run that stopped before reporting (unreadable manifest, broken
/// workspace) and examined no dependency. Output serde refuses is just as blind —
/// the findings it carried, if any, never reached comply.
fn read_report(stdout: &[u8], stderr: &[u8], workspace: &Path) -> Result<ShearReport> {
    if stdout.is_empty() {
        return Err(unusable(workspace, "reported nothing", &first_lines(stderr)));
    }
    serde_json::from_slice::<ShearReport>(stdout).map_err(|cause| {
        unusable(
            workspace,
            "emitted a report comply cannot read",
            &format!("{cause}\n{}", first_lines(stdout)),
        )
    })
}

/// A `cargo shear` run comply cannot trust, with the tool's own words.
fn unusable(workspace: &Path, what_failed: &str, detail: &str) -> anyhow::Error {
    crate::toolchain::ToolchainError::Unusable {
        what: format!(
            "cargo shear {what_failed} in {}; no dependency was examined. \
             Repair the workspace, then re-run comply.\n{detail}",
            workspace.display()
        ),
    }
    .into()
}

/// The opening lines of a tool's output, bounded for a refusal message.
fn first_lines(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw)
        .lines()
        .take(QUOTED_LINES)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// Check whether the dep named in a shear message is declared as
/// `optional = true` in the manifest. Optional deps are feature-gated
/// by design — flagging them as unused is noise because cargo-shear
/// only sees the default feature set.
fn is_optional_dep(manifest_path: &Path, message: &str) -> bool {
    let dep_name = message
        .split('`')
        .nth(1)
        .unwrap_or("");
    if dep_name.is_empty() {
        return false;
    }
    let Ok(content) = fs::read_to_string(manifest_path) else {
        return false;
    };
    let Ok(toml) = content.parse::<toml::Value>() else {
        return false;
    };
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(deps) = toml.get(section).and_then(|d| d.as_table())
            && let Some(entry) = deps.get(dep_name)
                && entry
                    .get("optional")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    return true;
                }
    }
    false
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
        let manifest_path = workspace.join(&file);
        if is_optional_dep(&manifest_path, &finding.message) {
            continue;
        }
        let offset = finding.location.map(|l| l.offset).unwrap_or(0);
        let (line, column) = byte_offset_to_line_col(&manifest_path, offset).unwrap_or((1, 1));
        diagnostics.push(Diagnostic {
            path: manifest_path.into(),
            line,
            column,
            rule_id: RULE_ID.into(),
            message: format!(
                "{} — every unused dep widens the supply chain, slows builds, \
                 and adds version-bump churn. Remove it from `[dependencies]` \
                 (or run `cargo shear --fix`).",
                finding.message
            ),
            severity: Severity::Error,
            span: None,
        });
    }
    Ok(diagnostics)
}

/// Read `path` and convert a byte `offset_bytes` into a 1-based
/// (line, column). Falls back to (1, 1) if the file can't be read —
/// better to surface the finding at the top of the manifest than to
/// drop it entirely.
fn byte_offset_to_line_col(path: &Path, offset_bytes: usize) -> Option<(usize, usize)> {
    let content = fs::read_to_string(path).ok()?;
    let bytes = content.as_bytes();
    let clamped = offset_bytes.min(bytes.len());
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

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
struct ShearReport {
    #[serde(default)]
    findings: Vec<Finding>,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
struct Finding {
    code: String,
    message: String,
    /// Optional — `shear/unlinked_files` findings have no file field.
    /// We skip those upstream so the absence is fine.
    #[serde(default)]
    file: Option<String>,
    /// Optional for the same reason.
    #[serde(default)]
    location: Option<Location>,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
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

    /// A `cargo shear` that stopped before reporting prints nothing on
    /// stdout: the clean run prints `{"findings":[]}`. Silence, and output
    /// comply cannot read, both used to yield zero findings — so an
    /// unreadable manifest read as "no unused dependency".
    #[test]
    fn a_shear_run_that_reported_nothing_is_refused() {
        let ws = Path::new("/ws");
        let clean = br#"{"summary":{"errors":0,"warnings":0,"fixed":0},"findings":[]}"#;
        assert!(read_report(clean, b"", ws).is_ok(), "a clean report stays a clean report");
        assert!(
            read_report(b"", b"error: failed to parse manifest", ws).is_err(),
            "empty stdout is a run that examined no dependency"
        );
        assert!(
            read_report(b"<html>502</html>", b"", ws).is_err(),
            "output comply cannot read is a run it cannot trust"
        );
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
