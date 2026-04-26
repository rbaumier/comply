//! regex-no-zero-quantifier TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! string literals that happen to contain `{0}` (Tailwind arbitrary
//! values, URLs, scoped import paths, format placeholders) cannot
//! false-positive as regex quantifiers.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Detect `{0}` or `{0,0}` quantifiers inside a regex pattern.
///
/// Both forms match exactly zero occurrences, making the quantified
/// sub-expression unreachable — almost always a typo for `{1}` or a
/// leftover from refactoring.
fn has_zero_quantifier(pattern: &str) -> bool {
    pattern.contains("{0}") || pattern.contains("{0,0}")
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_zero_quantifier(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-zero-quantifier",
        "Zero quantifier `{0}` or `{0,0}` matches nothing \u{2014} remove or fix the quantifier.".into(),
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
    fn flags_zero_quantifier() {
        assert_eq!(run_on("const re = /a{0}/;").len(), 1);
    }

    #[test]
    fn flags_zero_zero_quantifier() {
        assert_eq!(run_on("const re = /a{0,0}/;").len(), 1);
    }

    #[test]
    fn allows_positive_quantifier() {
        assert!(run_on("const re = /a{1}/;").is_empty());
    }

    #[test]
    fn allows_range_quantifier() {
        assert!(run_on("const re = /a{0,1}/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_with_zero_quantifier_lookalike() {
        let src = r#"const x = "grid-cols-[repeat(3,_minmax(0,_1fr))]{0}";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_with_zero_quantifier_lookalike() {
        let src = r#"const u = "https://example.com/path{0}";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_empty_string_with_quantifier_lookalike() {
        let src = r#"import x from "@scope/pkg/{0}";"#;
        assert!(run_on(src).is_empty());
    }
}
