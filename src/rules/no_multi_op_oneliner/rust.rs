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
}
