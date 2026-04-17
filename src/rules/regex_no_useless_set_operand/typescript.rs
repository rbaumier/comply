//! regex-no-useless-set-operand TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! Tailwind arbitrary-value classes, URLs, and scoped import paths in
//! string literals cannot false-positive as regex set-operation patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Detects useless operands in `v`-flag character class set operations.
/// Example: `[\d&&\w]` — `\d` is a subset of `\w`, so intersection is just `\d`.
/// Example: `[\w--\W]` — `\W` is the complement of `\w`, subtraction is useless.
fn has_useless_set_op(pattern: &str) -> bool {
    let complementary_pairs: &[(&str, &str)] = &[
        (r"\d", r"\w"), // \d is subset of \w
        (r"\w", r"\W"), // \w -- \W = \w
        (r"\d", r"\D"), // \d && \D = empty
        (r"\s", r"\S"), // \s && \S = empty
    ];

    for &(a, b) in complementary_pairs {
        let intersection = format!("[{a}&&{b}]");
        let intersection_rev = format!("[{b}&&{a}]");
        let subtraction = format!("[{a}--{b}]");
        let subtraction_rev = format!("[{b}--{a}]");

        if pattern.contains(&intersection)
            || pattern.contains(&intersection_rev)
            || pattern.contains(&subtraction)
            || pattern.contains(&subtraction_rev)
        {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "regex" {
        return;
    }
    let Some((pattern, flags)) = pattern_and_flags(&node, source) else { return };
    if !flags.contains('v') {
        return;
    }
    if !has_useless_set_op(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-useless-set-operand",
        "Useless operand in character class set operation.".into(),
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
    fn flags_subset_intersection() {
        assert_eq!(run_on(r#"const re = /[\d&&\w]/v;"#).len(), 1);
    }

    #[test]
    fn flags_complement_subtraction() {
        assert_eq!(run_on(r#"const re = /[\w--\W]/v;"#).len(), 1);
    }

    #[test]
    fn allows_non_v_flag() {
        assert!(run_on(r#"const re = /[\d]/g;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_empty() {
        let src = r#"import "";"#;
        assert!(run_on(src).is_empty());
    }
}
