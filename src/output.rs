//! Output formatter — renders diagnostics in ESLint-like single-line format.
//!
//! Format: `path:line:col: severity [rule-id] message`
//! One line per violation, easy to grep and parse by editors.

use crate::diagnostic::{Diagnostic, Severity};

/// Format diagnostics as ESLint-like single-line output.
pub fn format_eslint(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::with_capacity(diagnostics.len() * 120);
    for d in diagnostics {
        let severity = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        out.push_str(&format!(
            "{}:{}:{}: {} [{}] {}\n",
            d.path.display(),
            d.line,
            d.column,
            severity,
            d.rule_id,
            d.message,
        ));
    }
    out
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
}
