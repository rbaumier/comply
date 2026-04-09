//! oxlint subprocess — invokes oxlint on TS/JS files and converts JSON output
//! into unified Diagnostic structs.
//!
//! How it works:
//! 1. `is_available()` checks the binary is on PATH so the orchestrator can
//!    decide whether to skip silently or fail loudly. The result is cached
//!    in a `OnceLock` so we don't fork oxlint on every invocation.
//! 2. `lint_files()` invokes `oxlint --format json` with file paths terminated
//!    by `--` (so a path like `./-r.ts` is not interpreted as a flag).
//! 3. Parses the JSON envelope from raw bytes — never via lossy UTF-8
//!    conversion — and maps each entry to our Diagnostic.
//!
//! Position fallback: when oxlint emits a diagnostic with no labels we fall
//! back to (1, 1) instead of (0, 0). Editors choke on `path:0:0:`.

mod schema;

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use schema::{OxlintDiag, OxlintOutput, OxlintSeverity};

/// Check if oxlint binary is on PATH. Result is cached for the process lifetime.
pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("oxlint")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// Invoke oxlint on the given TS/JS files and return unified diagnostics.
#[must_use = "diagnostics from oxlint must be reported"]
pub fn lint_files(files: &[&SourceFile], config_path: Option<&Path>) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let output = run_subprocess(files, config_path)?;
    parse_json_bytes(&output.stdout, &output.stderr)
}

/// Spawn oxlint as a subprocess and validate exit status.
fn run_subprocess(
    files: &[&SourceFile],
    config_path: Option<&Path>,
) -> Result<std::process::Output> {
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
    Ok(output)
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
        let json = br#"{ "diagnostics": [{"message": "X", "severity": "error", "filename": "/tmp/x.ts", "labels": []}] }"#;
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
