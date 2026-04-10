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
}
