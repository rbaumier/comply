//! regex-sort-flags TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only. Reads the `flags` field
//! (e.g. `ig` in `/foo/ig`) and flags any sequence of 2+ flags that
//! is not in alphabetical order.
//!
//! Using the AST rather than raw text eliminates the false-positive
//! class where URLs (`"http://a/b"`), import paths
//! (`"@tanstack/react-query"`), and Tailwind arbitrary-value classes
//! look like `/pattern/flags`.

use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "regex" {
        return;
    }
    let Some(flags) = regex_flags(&node, source) else { return };
    if flags.len() < 2 {
        return;
    }
    let mut sorted: Vec<u8> = flags.bytes().collect();
    sorted.sort_unstable();
    if flags.as_bytes() == sorted.as_slice() {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-sort-flags",
        "Regex flags are not sorted alphabetically \u{2014} reorder them (e.g. `dgimsvy`).".into(),
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
    fn flags_unsorted_gi() {
        assert_eq!(run_on(r#"const re = /foo/ig;"#).len(), 1);
    }

    #[test]
    fn flags_unsorted_mig() {
        assert_eq!(run_on(r#"const re = /bar/mig;"#).len(), 1);
    }

    #[test]
    fn allows_sorted_flags() {
        assert!(run_on(r#"const re = /foo/gi;"#).is_empty());
    }

    #[test]
    fn allows_single_flag() {
        assert!(run_on(r#"const re = /foo/g;"#).is_empty());
    }

    #[test]
    fn allows_no_flags() {
        assert!(run_on(r#"const re = /foo/;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value() {
        let src = r#"const x = "has-[>svg]:grid";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_string() {
        let src = r#"const u = "http://a/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        let src = r#"import X from "@scope/pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
