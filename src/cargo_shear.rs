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
use rustc_hash::FxHashSet;
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
    let report =
        read_report(output.status.code(), &output.stdout, &output.stderr).map_err(|failure| {
            crate::toolchain::ToolchainError::Unusable {
                what: format!(
                    "cargo shear could not check {}; no dependency was checked.\n{failure}",
                    workspace.display()
                ),
            }
        })?;
    convert_findings(report.findings, workspace)
}

/// Cap, in characters, on the quoted `cargo shear` output.
const OUTPUT_EXCERPT_CHARS: usize = 2000;

/// Why `cargo shear` checked nothing.
#[derive(Debug, thiserror::Error)]
enum ShearFailure {
    #[error(
        "It exited with {}:\n{output}",
        .code.map_or_else(|| "a signal".to_owned(), |code| code.to_string())
    )]
    Exit { code: Option<i32>, output: String },
    #[error("Its report is unreadable ({source}):\n{output}")]
    Unreadable {
        source: serde_json::Error,
        output: String,
    },
}

/// The report of `cargo shear --format=json`.
/// Exit 0 means no finding, exit 1 some.
/// Any other code, or no report, fails.
/// Read as clean, it would hide unused dependencies.
fn read_report(
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<ShearReport, ShearFailure> {
    let excerpt = || -> String {
        let text = [stdout, stderr].map(|bytes| String::from_utf8_lossy(bytes).trim().to_owned());
        text.join("\n")
            .trim()
            .chars()
            .take(OUTPUT_EXCERPT_CHARS)
            .collect()
    };
    if !matches!(code, Some(0 | 1)) {
        return Err(ShearFailure::Exit {
            code,
            output: excerpt(),
        });
    }
    serde_json::from_slice(stdout).map_err(|source| ShearFailure::Unreadable {
        source,
        output: excerpt(),
    })
}

/// Check whether the dep named in a shear message is declared as
/// `optional = true` in the manifest. Optional deps are feature-gated
/// by design — flagging them as unused is noise because cargo-shear
/// only sees the default feature set.
fn is_optional_dep(manifest_path: &Path, message: &str) -> bool {
    let dep_name = message.split('`').nth(1).unwrap_or("");
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
/// A report without `findings` is unreadable, not clean.
// unknown-fields-allowed: cargo-shear's JSON report, which also carries `summary`
#[derive(Debug, Deserialize)]
struct ShearReport {
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

    #[test]
    fn reads_the_report_of_exit_codes_0_and_1() {
        let clean = br#"{"findings":[]}"#;
        let found = br#"{"findings":[{"code":"shear/unused_dependency","message":"unused dependency `ok`"}]}"#;
        assert!(
            read_report(Some(0), clean, b"")
                .unwrap()
                .findings
                .is_empty()
        );
        assert_eq!(read_report(Some(1), found, b"").unwrap().findings.len(), 1);
    }

    #[test]
    fn refuses_a_metadata_error_with_exit_code_2() {
        // Measured with cargo-shear 1.14, on a missing path dependency.
        let stdout = b"error: Metadata error: `cargo metadata` exited with an error: \n";
        let stderr = b"error: no matching package named `nope` found\n";
        let detail = read_report(Some(2), stdout, stderr)
            .unwrap_err()
            .to_string();
        assert!(detail.starts_with("It exited with 2:"), "{detail}");
        assert!(detail.contains("Metadata error"), "{detail}");
        assert!(
            detail.contains("no matching package named `nope`"),
            "{detail}"
        );
    }

    #[test]
    fn refuses_an_empty_or_unreadable_report() {
        for stdout in [
            &b""[..],
            b"warning: x\n{\"findings\":[]}",
            br#"{"summary":{}}"#,
        ] {
            let detail = read_report(Some(0), stdout, b"").unwrap_err().to_string();
            assert!(detail.starts_with("Its report is unreadable"), "{detail}");
        }
        assert!(read_report(None, br#"{"findings":[]}"#, b"").is_err());
    }

    #[test]
    fn bounds_the_quoted_output() {
        let stderr = "e".repeat(OUTPUT_EXCERPT_CHARS.saturating_mul(2));
        let detail = read_report(Some(101), b"", stderr.as_bytes())
            .unwrap_err()
            .to_string();
        let header = "It exited with 101:\n".len();
        assert_eq!(detail.len(), header.saturating_add(OUTPUT_EXCERPT_CHARS));
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
