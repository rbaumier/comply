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

impl Diagnostic {
    /// Build a diagnostic anchored on a tree-sitter node. Captures both the
    /// human-friendly `(line, column)` via `node.start_position()` and the
    /// byte `span` via `node.byte_range()` so the pretty renderer can
    /// highlight the exact source range — not just the whole line.
    ///
    /// Native rules should prefer this over constructing a `Diagnostic { .. }`
    /// literal. Delegated diagnostics (oxlint/clippy/knip/madge) only have
    /// `(line, col)` from external JSON output and stay on the literal form
    /// with `span: None`; the renderer falls back to whole-line highlighting
    /// for those.
    #[must_use]
    pub fn at_node(
        path: &std::path::Path,
        node: &tree_sitter::Node<'_>,
        rule_id: &str,
        message: String,
        severity: Severity,
    ) -> Self {
        let pos = node.start_position();
        let range = node.byte_range();
        Self {
            path: path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: rule_id.into(),
            message,
            severity,
            span: Some((range.start, range.len())),
        }
    }
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

    #[test]
    fn at_node_captures_byte_range_line_and_column() {
        let source = "const x = 1;";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("set tree-sitter-typescript language");
        let tree = parser
            .parse(source, None)
            .expect("parse const x = 1; as TypeScript");
        let root = tree.root_node();
        // The first child of the root is the lexical_declaration `const x = 1;`.
        let decl = root.child(0).expect("root should have a first child");

        let diag = Diagnostic::at_node(
            std::path::Path::new("fixture.ts"),
            &decl,
            "test-rule",
            "body".into(),
            Severity::Warning,
        );

        assert_eq!(diag.path, std::path::PathBuf::from("fixture.ts"));
        assert_eq!(diag.rule_id, "test-rule");
        assert_eq!(diag.message, "body");
        assert_eq!(diag.line, 1);
        assert_eq!(diag.column, 1);
        // The lexical_declaration spans the entire string.
        assert_eq!(diag.span, Some((0, source.len())));
    }
}
