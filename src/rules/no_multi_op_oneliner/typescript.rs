//! no-multi-op-oneliner backend — reject single lines with 4+ chained
//! operators crammed together.
//!
//! Why: `const x = items.filter(i => i.active).map(i => i.price).reduce((a,b) => a+b, 0) * tax + discount;`
//! is unreadable. Extract intermediate named variables — `active`,
//! `prices`, `subtotal`, `total` — so each step's purpose is visible.
//!
//! Detection: for every `expression_statement` / `variable_declarator`
//! that spans a single line, count the operator-like tokens on that line
//! (call parens, binary operators, member access dots). Flag when the
//! count crosses the threshold.
//!
//! This is a heuristic. It deliberately prefers false negatives over
//! false positives: mundane one-liners don't trip it.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        super::dense_lines::scan_dense_lines(
            ctx,
            tree,
            &["expression_statement", "variable_declarator"],
            &["comment"],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_heavy_oneliner() {
        let source = "const total = items.filter(i => i.active).map(i => i.price).reduce((a, b) => a + b, 0) * tax + discount;";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_simple_oneliner() {
        assert!(run_on("const x = a + b;").is_empty());
    }

    #[test]
    fn allows_short_but_dense_expression() {
        // Dense but short — under the line-length floor.
        assert!(run_on("const x = a.b.c + d.e * f;").is_empty());
    }

    #[test]
    fn does_not_count_operators_inside_trailing_line_comment() {
        // TS equivalent of the Rust FP from RULES_TO_FIX.md #6.
        let source =
            "expect(run(\"utils.spec.ts\", \"// TODO: add tests\").length).toBe(1); // eslint-disable-next-line — test content not a real marker";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_count_operators_inside_trailing_block_comment() {
        let source = "const x = a + b; /* note: a - b * c / d - e + f */";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_dense_code_with_trailing_comment() {
        let source = "const total = items.filter(i => i.active).map(i => i.price).reduce((a, b) => a + b, 0) * tax + discount; // total";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn does_not_flag_short_code_with_long_trailing_comment() {
        let source = "const x = a + b; // a fairly long explanation that the result is the sum of a and b and not anything more interesting at all";
        assert!(run_on(source).is_empty());
    }
}
