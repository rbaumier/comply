//! regex-no-non-standard-flag TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only. Reads the `flags` field
//! (e.g. `gi` in `/foo/gi`) and flags any character outside the
//! ECMAScript spec set `d g i m s u v y`.
//!
//! Using the AST rather than raw text eliminates the false-positive
//! class where URLs (`"http://a/b"`), import paths
//! (`"@tanstack/react-query"`), and Tailwind arbitrary-value classes
//! look like `/pattern/flags`.

use crate::diagnostic::{Diagnostic, Severity};

const STANDARD_FLAGS: &[u8] = b"dgimsuvy";

/// Extract the flags substring of a tree-sitter `regex` node.
///
/// Prefers the `flags` field; falls back to manually parsing the
/// node's text as `/pattern/flags` if the field isn't exposed by
/// the grammar version in use.
fn regex_flags<'a>(node: &tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    if let Some(flags_node) = node.child_by_field_name("flags")
        && let Ok(t) = flags_node.utf8_text(source)
    {
        return Some(t);
    }
    let full = node.utf8_text(source).ok()?;
    let inner = full.strip_prefix('/')?;
    let last_slash = inner.rfind('/')?;
    Some(&inner[last_slash + 1..])
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some(flags) = regex_flags(&node, source) else { return };
    if flags.is_empty() {
        return;
    }
    if flags.bytes().all(|f| STANDARD_FLAGS.contains(&f)) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-non-standard-flag",
        "Non-standard regex flag detected \u{2014} standard flags are: d, g, i, m, s, u, v, y.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_non_standard_flag() {
        assert_eq!(run_on(r#"const re = /foo/x;"#).len(), 1);
    }

    #[test]
    fn allows_standard_flags() {
        assert!(run_on(r#"const re = /foo/gim;"#).is_empty());
    }

    #[test]
    fn flags_unknown_flag_l() {
        assert_eq!(run_on(r#"const re = /bar/l;"#).len(), 1);
    }

    #[test]
    fn allows_no_flags() {
        assert!(run_on(r#"const re = /foo/;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_url_with_y_segment() {
        // /query was flagged as `q` flag under the text-based impl.
        let src = r#"const u = "http://localhost:6762/api/v1/diffs/query";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_import_path_with_y() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_tailwind_arbitrary_value() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }
}
