//! Diagnostic model — unified representation of a single lint violation.
//!
//! Every source (oxlint, clippy, custom rules) converts its findings into
//! this struct so the output formatter can treat them uniformly.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;

/// A single lint violation with location, rule, and remediation message.
///
/// `path` is `Arc<Path>` so the same path is shared by every diagnostic
/// emitted from one file — zero per-diagnostic allocation when the engine
/// builds the Arc once per file. `rule_id` is `Cow<'static, str>` so most
/// rules (which use `"some-id".into()` on a `&'static str`) get a
/// zero-alloc `Cow::Borrowed`; only the rare dynamic-rule-id sources
/// (clippy remap, ignore-comment parser) pay the `Cow::Owned` cost.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    #[serde(with = "arc_path_serde")]
    pub path: Arc<Path>,
    pub line: usize,
    pub column: usize,
    pub rule_id: Cow<'static, str>,
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
        path: impl Into<Arc<Path>>,
        node: &tree_sitter::Node<'_>,
        rule_id: &'static str,
        message: String,
        severity: Severity,
    ) -> Self {
        let pos = node.start_position();
        let range = node.byte_range();
        Self {
            path: path.into(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: Cow::Borrowed(rule_id),
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

mod arc_path_serde {
    use super::{Arc, Path};
    use serde::{Deserialize, Deserializer, Serializer};
    use std::path::PathBuf;

    pub fn serialize<S: Serializer>(path: &Arc<Path>, s: S) -> Result<S::Ok, S::Error> {
        use serde::Serialize;
        // PathBuf implements Serialize; Path does not directly, but we can
        // serialize by going through the OsStr / lossy form. PathBuf is the
        // standard route — we hold an &Path so wrap it in a transient Path
        // newtype via PathBuf reference cost (Path::serialize is provided
        // by serde as an impl on Path).
        // serde provides `impl Serialize for Path`.
        Path::serialize(path.as_ref(), s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Path>, D::Error> {
        let buf = PathBuf::deserialize(d)?;
        Ok(Arc::from(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arc_path(s: &str) -> Arc<Path> {
        Arc::from(Path::new(s))
    }

    #[test]
    fn diagnostic_serializes_span_when_present() {
        let d = Diagnostic {
            path: arc_path("f.rs"),
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
            path: arc_path("f.rs"),
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
            arc_path("fixture.ts"),
            &decl,
            "test-rule",
            "body".into(),
            Severity::Warning,
        );

        assert_eq!(&*diag.path, Path::new("fixture.ts"));
        assert_eq!(diag.rule_id, "test-rule");
        assert_eq!(diag.message, "body");
        assert_eq!(diag.line, 1);
        assert_eq!(diag.column, 1);
        // The lexical_declaration spans the entire string.
        assert_eq!(diag.span, Some((0, source.len())));
    }
}
