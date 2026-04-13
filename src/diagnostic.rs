//! Diagnostic model — unified representation of a single lint violation.
//!
//! Every source (oxlint, clippy, custom rules) converts its findings into
//! this struct so the output formatter can treat them uniformly.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single lint violation with location, rule, and remediation message.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub rule_id: String,
    pub message: String,
    pub severity: Severity,
    /// Byte range into the source file, `(offset, length)`. Populated by
    /// native tree-sitter rules that have the node in scope. `None` for
    /// delegated diagnostics (oxlint/clippy/knip/madge) — the renderer
    /// falls back to whole-line highlighting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<(usize, usize)>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_serializes_span_when_present() {
        let d = Diagnostic {
            path: std::path::PathBuf::from("f.rs"),
            line: 1,
            column: 1,
            rule_id: "r".into(),
            message: "m".into(),
            severity: Severity::Warning,
            span: Some((10, 5)),
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"span\""));
    }

    #[test]
    fn diagnostic_omits_span_when_absent() {
        let d = Diagnostic {
            path: std::path::PathBuf::from("f.rs"),
            line: 1,
            column: 1,
            rule_id: "r".into(),
            message: "m".into(),
            severity: Severity::Warning,
            span: None,
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("\"span\""));
    }
}
