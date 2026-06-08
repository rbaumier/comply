//! for-loop-increment-sign Rust backend.
//!
//! Rust doesn't have C-style `for (init; cond; update)` loops.
//! `for x in ...` loops use iterators, so the concept of "wrong increment
//! direction" doesn't apply. This is a no-op stub for completeness.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
#[allow(dead_code)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, _ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        // Rust for-in loops use iterators; there's no increment direction to check.
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
    fn allows_for_in_loop() {
        assert!(run_on("fn f() { for x in 0..10 { println!(\"{}\", x); } }").is_empty());
    }

    #[test]
    fn allows_reverse_range() {
        assert!(run_on("fn f() { for x in (0..10).rev() {} }").is_empty());
    }
}
