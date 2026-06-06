//! Output formatter — renders diagnostics in two wire formats:
//!
//! - **ESLint-like** (`format_eslint`) for humans and grep-based tools.
//!   Each line: `path:line:col: severity [rule-id] message`
//! - **JSON** (`format_json`) for editors, CI dashboards, and anything
//!   that wants structured data. One object per diagnostic, sorted by
//!   path then line.
//!
//! A third, miette-powered pretty renderer lives in the `pretty` submodule
//! and is re-exported as `render_pretty`. The shared line/col→byte-span
//! resolver it depends on lives in `span_resolver` and is used only by
//! `pretty` — intentionally module-private.

mod pretty;
mod span_resolver;

pub use pretty::render_pretty;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::diagnostic::{Diagnostic, Severity};
use std::fmt::Write as _;

/// Average bytes per ESLint-line — used to pre-size the output buffer to
/// avoid log-N reallocations on long diagnostic lists. Picked from observation:
/// `path:line:col: error [rule-id] message` averages ~120 bytes in practice.
const BYTES_PER_LINE_HINT: usize = 120;

/// Appends one diagnostic as an eslint-like single line. Shared between
/// `format_eslint` (the public piped/CI formatter) and the pretty renderer's
/// unreadable-file fallback path so both wire formats stay byte-identical.
pub(super) fn write_eslint_line(out: &mut String, d: &Diagnostic) {
    let severity = match d.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    // Writing to a String via fmt::Write is infallible.
    writeln!(
        out,
        "{}:{}:{}: {} [{}] {}",
        d.path.display(),
        d.line,
        d.column,
        severity,
        d.rule_id,
        d.message,
    )
    .expect("fmt::Write into String is infallible");
}

/// Format diagnostics as ESLint-like single-line output.
pub fn format_eslint(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::with_capacity(diagnostics.len() * BYTES_PER_LINE_HINT);
    for diag in diagnostics {
        write_eslint_line(&mut out, diag);
    }
    out
}

/// Format diagnostics as a JSON array — one object per violation.
/// Stable shape so editors and CI tools can depend on it.
pub fn format_json(diagnostics: &[Diagnostic]) -> Result<String> {
    // Serialize borrowing structs directly instead of building an intermediate
    // `serde_json::Value` tree (one heap-allocated map per diagnostic). Field
    // order matches the historical insertion order so the wire bytes are
    // unchanged. `collect_str` streams the path's lossy display form straight
    // into the serializer — no per-diagnostic `String` allocation.
    fn serialize_display_path<S: serde::Serializer>(
        path: &std::path::Path,
        s: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(&path.display())
    }

    #[derive(Serialize)]
    struct JsonDiag<'a> {
        #[serde(serialize_with = "serialize_display_path")]
        path: &'a std::path::Path,
        line: usize,
        column: usize,
        #[serde(rename = "ruleId")]
        rule_id: &'a str,
        message: &'a str,
        severity: &'static str,
    }

    let payload: Vec<JsonDiag> = diagnostics
        .iter()
        .map(|d| JsonDiag {
            path: d.path.as_ref(),
            line: d.line,
            column: d.column,
            rule_id: d.rule_id.as_ref(),
            message: d.message.as_ref(),
            severity: match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            },
        })
        .collect();
    serde_json::to_string_pretty(&payload).context("failed to serialize diagnostics as JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn diag(severity: Severity) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(Path::new("foo.ts")),
            line: 10,
            column: 5,
            rule_id: "no-throw".into(),
            message: "use Result".into(),
            severity,
            span: None,
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
