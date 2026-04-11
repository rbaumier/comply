//! no-useless-increment Rust backend.
//!
//! Rust doesn't have `x++`/`x--` operators, so this rule doesn't
//! directly apply. However, we can flag patterns like `return { x += 1; x - 1 }`
//! which is the closest Rust equivalent. For now, this is a no-op stub
//! that keeps the Rust backend registered for completeness.
//! The rule essentially never fires in Rust since postfix increment doesn't exist.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, _ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        // Rust has no postfix `++`/`--` operators, so this rule never fires.
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn allows_normal_increment() {
        assert!(run_on("fn f() { let mut x = 0; x += 1; }").is_empty());
    }

    #[test]
    fn allows_return() {
        assert!(run_on("fn f() -> i32 { return 42; }").is_empty());
    }
}
