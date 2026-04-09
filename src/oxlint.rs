//! oxlint subprocess — invokes oxlint on TS/JS files and converts JSON output
//! into unified Diagnostic structs.
//!
//! How it works:
//! 1. `is_available()` checks the binary is on PATH so the orchestrator can
//!    decide whether to skip silently or fail loudly.
//! 2. `lint_files()` invokes `oxlint --format json` with file paths terminated
//!    by `--` (so a path like `./-r.ts` is not interpreted as a flag).
//! 3. Parses the JSON envelope (`diagnostics` array) from raw bytes — never
//!    via lossy UTF-8 conversion — and maps each entry to our Diagnostic.
//!
//! Position fallback: when oxlint emits a diagnostic with no labels we fall
//! back to (1, 1) instead of (0, 0). Editors choke on `path:0:0:`.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;

/// Top-level oxlint JSON output envelope.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct OxlintOutput {
    #[serde(default)]
    diagnostics: Vec<OxlintDiag>,
    /// Fields oxlint emits that we currently ignore — listed so the
    /// `deny_unknown_fields` contract above doesn't reject the payload.
    #[serde(default, rename = "number_of_files")]
    _number_of_files: Option<u64>,
    #[serde(default, rename = "number_of_rules")]
    _number_of_rules: Option<u64>,
    #[serde(default, rename = "threads_count")]
    _threads_count: Option<u64>,
    #[serde(default, rename = "start_time")]
    _start_time: Option<f64>,
}

/// A single oxlint diagnostic — adapted from actual oxlint 1.59 JSON format.
///
/// Some fields are accepted-but-unused so `deny_unknown_fields` doesn't reject
/// the payload when oxlint emits them. They're marked `#[allow(dead_code)]`.
#[derive(Deserialize)]
#[allow(dead_code)]
struct OxlintDiag {
    #[serde(default)]
    message: String,
    /// Rule identifier, e.g. "eslint(no-unused-vars)".
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    severity: OxlintSeverity,
    #[serde(default)]
    filename: String,
    /// Position labels — first label carries the primary span.
    #[serde(default)]
    labels: Vec<OxlintLabel>,
    // Fields we don't use but oxlint emits — accept and discard.
    #[serde(default)]
    causes: Vec<serde_json::Value>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    help: Option<String>,
    #[serde(default)]
    related: Vec<serde_json::Value>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum OxlintSeverity {
    #[default]
    Error,
    Warning,
    Advice,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct OxlintLabel {
    #[serde(default)]
    span: OxlintSpan,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Deserialize, Default)]
#[allow(dead_code)]
struct OxlintSpan {
    #[serde(default)]
    line: usize,
    #[serde(default)]
    column: usize,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    length: usize,
}

/// Check if oxlint binary is on PATH.
pub fn is_available() -> bool {
    Command::new("oxlint")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Invoke oxlint on the given TS/JS files and return unified diagnostics.
pub fn lint_files(files: &[&SourceFile], config_path: Option<&Path>) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }

    let mut cmd = Command::new("oxlint");
    cmd.args(["--format", "json"]);
    if let Some(cfg) = config_path {
        cmd.arg("-c").arg(cfg);
    }
    // `--` terminates option parsing so file paths starting with `-` are not
    // interpreted as flags by oxlint.
    cmd.arg("--");
    for f in files {
        cmd.arg(&f.path);
    }

    let output = cmd
        .output()
        .context("failed to invoke oxlint — install it with: npm install -g oxlint")?;

    // oxlint exits 1 when violations are found — that is normal, not an error.
    if !output.status.success() && output.status.code() != Some(1) {
        bail!(
            "oxlint crashed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    parse_json_bytes(&output.stdout, &output.stderr)
}

/// Parse oxlint JSON output bytes into unified Diagnostic structs.
/// Includes stderr in the error context so the user sees what went wrong.
fn parse_json_bytes(stdout: &[u8], stderr: &[u8]) -> Result<Vec<Diagnostic>> {
    let envelope: OxlintOutput = serde_json::from_slice(stdout).with_context(|| {
        format!(
            "failed to parse oxlint JSON output. oxlint stderr: {}",
            String::from_utf8_lossy(stderr)
        )
    })?;

    Ok(envelope.diagnostics.into_iter().map(into_diagnostic).collect())
}

/// Convert one oxlint diagnostic into our unified format.
fn into_diagnostic(d: OxlintDiag) -> Diagnostic {
    let (line, column) = d
        .labels
        .first()
        .map(|l| (l.span.line.max(1), l.span.column.max(1)))
        .unwrap_or((1, 1));

    let severity = match d.severity {
        OxlintSeverity::Warning | OxlintSeverity::Advice => Severity::Warning,
        OxlintSeverity::Error => Severity::Error,
    };

    Diagnostic {
        path: d.filename.into(),
        line,
        column,
        rule_id: d.code.unwrap_or_else(|| "oxlint/unknown".into()),
        message: d.message,
        severity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_oxlint_output() {
        // Captured from `oxlint --format json` on a file with `any` type.
        let json = br#"{ "diagnostics": [{"message": "Test", "code": "eslint(test)", "severity": "warning", "causes": [], "filename": "/tmp/x.ts", "labels": [{"label": "x", "span": {"offset": 6, "length": 1, "line": 3, "column": 5}}], "related": []}], "number_of_files": 1, "number_of_rules": 10, "threads_count": 4, "start_time": 0.001 }"#;
        let result = parse_json_bytes(json, b"").expect("must parse");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert_eq!(result[0].column, 5);
        assert_eq!(result[0].rule_id, "eslint(test)");
    }

    #[test]
    fn fallback_position_is_one_one_not_zero_zero() {
        let json = br#"{ "diagnostics": [{"message": "X", "severity": "error", "filename": "/tmp/x.ts", "labels": [], "causes": [], "related": []}] }"#;
        let result = parse_json_bytes(json, b"").expect("must parse");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 1);
    }

    #[test]
    fn empty_diagnostics_array_yields_empty_vec() {
        let json = br#"{ "diagnostics": [] }"#;
        let result = parse_json_bytes(json, b"").expect("must parse");
        assert!(result.is_empty());
    }
}
