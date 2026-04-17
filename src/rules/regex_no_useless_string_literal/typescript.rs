//! regex-no-useless-string-literal TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so URLs,
//! Tailwind arbitrary-value classes, and import paths inside string literals
//! cannot false-positive as regex literals.
//!
//! Flags `\q{X|Y}` string disjunctions inside `v`-flag character classes
//! where every alternative is a single character, because the disjunction
//! can be simplified to a plain character class element.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Returns true when `pattern` contains a `\q{a|b|...}` disjunction whose
/// alternatives are all exactly one character long.
fn has_single_char_string_disjunction(pattern: &str) -> bool {
    let mut search_from = 0;
    while let Some(pos) = pattern[search_from..].find("\\q{") {
        let start = search_from + pos + 3;
        if let Some(end) = pattern[start..].find('}') {
            let content = &pattern[start..start + end];
            let parts: Vec<&str> = content.split('|').collect();
            if parts.len() >= 2 && parts.iter().all(|p| p.chars().count() == 1) {
                return true;
            }
            search_from = start + end + 1;
        } else {
            break;
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
    if !has_single_char_string_disjunction(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-useless-string-literal",
        "String disjunction of single characters can be simplified to a character class.".into(),
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
    fn flags_single_char_disjunction() {
        assert_eq!(run_on(r#"const re = /[\q{a|b}]/v;"#).len(), 1);
    }

    #[test]
    fn allows_multi_char_string() {
        assert!(run_on(r#"const re = /[\q{ab|cd}]/v;"#).is_empty());
    }

    #[test]
    fn allows_non_v_flag() {
        assert!(run_on(r#"const re = /foo/g;"#).is_empty());
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
        let src = r#"import X from "@scope/pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
