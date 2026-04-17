//! regex-no-empty-string-literal-v TypeScript / JavaScript / TSX backend.
//!
//! Flags empty `\q{}` string-literal quantifier inside a `v`-flag
//! character class. AST-only detection eliminates FPs from ordinary
//! strings that happen to contain the `\q{}` sequence.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn has_empty_string_literal_v(pattern: &str, flags: &str) -> bool {
    flags.contains('v') && pattern.contains("\\q{}")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, flags)) = pattern_and_flags(&node, source) else { return };
    if !has_empty_string_literal_v(pattern, flags) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-empty-string-literal-v",
        "Empty string literal in v-flag character class is unexpected.".into(),
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
    fn flags_empty_q_in_v_flag() {
        assert_eq!(run_on(r#"const re = /[\q{}]/v;"#).len(), 1);
    }

    #[test]
    fn allows_non_v_flag() {
        assert!(run_on(r#"const re = /[\q{}]/g;"#).is_empty());
    }

    #[test]
    fn allows_non_empty_q() {
        assert!(run_on(r#"const re = /[\q{ab}]/v;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#).is_empty());
    }

    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b/c";"#).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }
}
