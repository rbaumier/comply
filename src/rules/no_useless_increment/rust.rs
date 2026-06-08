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
#[allow(dead_code)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, _ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        // Rust has no postfix `++`/`--` operators, so this rule never fires.
        Vec::new()
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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
