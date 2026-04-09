//! Output formatter — renders diagnostics in ESLint-like single-line format.
//!
//! Format: `path:line:col: severity [rule-id] message`
//! One line per violation, easy to grep and parse by editors.

use crate::diagnostic::{Diagnostic, Severity};

/// Format diagnostics as ESLint-like single-line output.
#[allow(dead_code)] // Called by main orchestrator (task 12).
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
