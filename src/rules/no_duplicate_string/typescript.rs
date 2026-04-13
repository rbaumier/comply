//! no-duplicate-string — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;
use crate::rules::walker::collect_nodes_of_kinds;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        super::collect_diagnostics(tree, ctx, TS_STRING_KINDS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_string_appearing_three_times() {
        let src = r#"
            const a = "hello world";
            const b = "hello world";
            const c = "hello world";
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_fourth_occurrence_too() {
        let src = r#"
            const a = "repeated str";
            const b = "repeated str";
            const c = "repeated str";
            const d = "repeated str";
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn ignores_short_strings() {
        let src = r#"
            const a = "short";
            const b = "short";
            const c = "short";
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_two_occurrences() {
        let src = r#"
            const a = "long enough string";
            const b = "long enough string";
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_string_in_comment() {
        // The literal `"structured_output"` appears once as a real
        // string but many times in comments — comments are not
        // visited, so the count stays at 1.
        let src = r#"
            // the "structured_output" field is different from "result"
            // we always check "structured_output" first
            // and fall back if "structured_output" is missing
            const FIELD = "structured_output";
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_inner_quotes_of_template_string() {
        // The template contains `${var}` but its content is ONE AST
        // node, so it counts as one occurrence.
        let src = r#"
            const x = `outer "inner" outer`;
            const y = `another "inner" another`;
            const z = `final "inner" final`;
        "#;
        assert!(run(src).is_empty());
    }
}
