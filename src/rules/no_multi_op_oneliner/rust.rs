//! no-multi-op-oneliner backend for Rust.
//!
//! Delegates the entire detection to `super::dense_lines::scan_dense_lines` —
//! the only Rust-specific knowledge is the set of AST node kinds that
//! count as candidates (`let_declaration` and `expression_statement`).

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        super::dense_lines::scan_dense_lines(
            ctx,
            tree,
            &["let_declaration", "expression_statement"],
            &["line_comment", "block_comment"],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_heavy_oneliner() {
        let source = "fn f() { let total = items.iter().filter(|i| i.active).map(|i| i.price).sum::<f64>() * tax + discount; }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_simple_oneliner() {
        assert!(run_on("fn f() { let x = a + b; }").is_empty());
    }

    #[test]
    fn allows_short_but_dense_expression() {
        // Dense but short — under the line-length floor.
        assert!(run_on("fn f() { let x = a.b.c + d.e * f; }").is_empty());
    }

    #[test]
    fn does_not_count_operators_inside_trailing_line_comment() {
        // The exact false positive from RULES_TO_FIX.md #6: a test
        // assertion with a trailing `// comply-ignore: …` comment. The
        // hyphens, slashes and dots inside the comment must NOT be
        // counted as code operators.
        let source = "#[test]\nfn t() {\n    assert_eq!(run(\"utils.spec.ts\", \"// TODO: add tests\").len(), 1); // comply-ignore: todo-needs-issue-link — test content, not a real marker.\n}";
        assert!(
            run_on(source).is_empty(),
            "trailing line comment must not contribute operators or length"
        );
    }

    #[test]
    fn does_not_count_operators_inside_trailing_block_comment() {
        let source = "fn f() { let x = a + b; /* note: this is - + - + - + - + a comment */ }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_dense_code_with_trailing_comment() {
        // Multi-line so the closing `}` isn't swallowed by the trailing
        // line comment.
        let source = "fn f() {\n    let total = items.iter().filter(|i| i.active).map(|i| i.price).sum::<f64>() * tax + discount; // total\n}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn does_not_flag_short_code_with_long_trailing_comment() {
        let source = "fn f() {\n    let x = a + b; // a fairly long explanation that the result is the sum of a and b and not anything more interesting\n}";
        assert!(run_on(source).is_empty());
    }
}
