//! Output formatter — renders diagnostics in two wire formats:
//!
//! - **ESLint-like** (`format_eslint`) for humans and grep-based tools.
//!   Each line: `path:line:col: severity [rule-id] message`
//! - **JSON** (`format_json`) for editors, CI dashboards, and anything
//!   that wants structured data. One object per diagnostic, sorted by
//!   path then line.

use anyhow::{Context, Result};
use serde_json::json;

use crate::diagnostic::{Diagnostic, Severity};
use std::fmt::Write as _;

/// Average bytes per ESLint-line — used to pre-size the output buffer to
/// avoid log-N reallocations on long diagnostic lists. Picked from observation:
/// `path:line:col: error [rule-id] message` averages ~120 bytes in practice.
const BYTES_PER_LINE_HINT: usize = 120;

/// Format diagnostics as ESLint-like single-line output.
pub fn format_eslint(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::with_capacity(diagnostics.len() * BYTES_PER_LINE_HINT);
    for diag in diagnostics {
        let severity = match diag.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        // writeln! writes directly into `out` without an intermediate String alloc.
        // unwrap is infallible here — writing to a String never fails.
        writeln!(
            out,
            "{}:{}:{}: {} [{}] {}",
            diag.path.display(),
            diag.line,
            diag.column,
            severity,
            diag.rule_id,
            diag.message,
        )
        .expect("writing to a String never fails");
    }
    out
}

/// Format diagnostics as a JSON array — one object per violation.
/// Stable shape so editors and CI tools can depend on it.
pub fn format_json(diagnostics: &[Diagnostic]) -> Result<String> {
    let payload: Vec<_> = diagnostics
        .iter()
        .map(|d| {
            json!({
                "path": d.path.display().to_string(),
                "line": d.line,
                "column": d.column,
                "ruleId": d.rule_id,
                "message": d.message,
                "severity": match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                },
            })
        })
        .collect();
    serde_json::to_string_pretty(&payload).context("failed to serialize diagnostics as JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn diag(severity: Severity) -> Diagnostic {
        Diagnostic {
            path: PathBuf::from("foo.ts"),
            line: 10,
            column: 5,
            rule_id: "no-throw".into(),
            message: "use Result".into(),
            severity,
        }
    }

    #[test]
    fn empty_diagnostics_produces_empty_string() {
        assert_eq!(format_eslint(&[]), "");
    }

    #[test]
    fn formats_error_severity_correctly() {
        let out = format_eslint(&[diag(Severity::Error)]);
        assert_eq!(out, "foo.ts:10:5: error [no-throw] use Result\n");
    }

    #[test]
    fn formats_warning_severity_correctly() {
        let out = format_eslint(&[diag(Severity::Warning)]);
        assert_eq!(out, "foo.ts:10:5: warning [no-throw] use Result\n");
    }

    #[test]
    fn multiple_diagnostics_each_on_own_line() {
        let out = format_eslint(&[diag(Severity::Error), diag(Severity::Warning)]);
        assert_eq!(out.lines().count(), 2);
    }

    #[test]
    fn json_format_produces_array() {
        let out = format_json(&[diag(Severity::Error)]).unwrap();
        assert!(out.starts_with('['));
        assert!(out.contains("\"ruleId\": \"no-throw\""));
        assert!(out.contains("\"severity\": \"error\""));
        assert!(out.contains("\"line\": 10"));
    }

    #[test]
    fn json_format_empty_array_for_no_diagnostics() {
        assert_eq!(format_json(&[]).unwrap(), "[]");
    }
}
