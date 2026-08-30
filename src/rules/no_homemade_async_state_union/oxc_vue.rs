//! no-homemade-async-state-union Vue SFC backend (oxc-based).
//!
//! Extracts the `<script>` blocks with tree-sitter-vue, then hands each one to
//! `vue_sfc_oxc::run_oxc_check_on_vue_block`, which parses it with oxc_parser
//! and runs the TypeScript check — a Vue component mirrors request state in
//! the same `ref<"idle" | "loading">()` / `{ data, loading, error }` shapes.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::{vue_sfc, vue_sfc_oxc};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let blocks = vue_sfc::extract_scripts(tree, ctx.source);
        let mut diagnostics = Vec::new();
        for block in &blocks {
            vue_sfc_oxc::run_oxc_check_on_vue_block(
                block,
                &super::oxc_typescript::Check,
                ctx,
                &mut diagnostics,
            );
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parse");
        let path = PathBuf::from("t.vue");
        let ctx = CheckCtx::for_test(&path, source);
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_a_hand_rolled_union_in_a_vue_script() {
        let source = "<script setup lang=\"ts\">\ntype Status = \"idle\" | \"loading\";\n</script>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_the_triplet_in_a_vue_script() {
        let source = "<script setup lang=\"ts\">\ninterface S { data: string; loading: boolean; error: Error | null; }\n</script>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_business_state_in_a_vue_script() {
        let source =
            "<script setup lang=\"ts\">\ntype OrderState = \"pending\" | \"shipped\";\n</script>";
        assert!(run(source).is_empty(), "expected no diagnostics");
    }
}
