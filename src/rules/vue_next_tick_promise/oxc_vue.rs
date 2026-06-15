//! vue-next-tick-promise Vue SFC backend (oxc-based).
//!
//! Extracts `<script>` / `<script setup>` blocks with tree-sitter-vue, then
//! delegates to `vue_sfc_oxc::run_oxc_check_on_vue_block`, which re-parses each
//! block with oxc and runs the shared `oxc_typescript::Check`.

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
    fn flags_next_tick_callback_in_script() {
        let src =
            "<script>\nimport { nextTick } from \"vue\";\nnextTick(() => {\n  updateDom();\n});\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_await_next_tick_in_script() {
        let src = "<script setup>\nimport { nextTick } from \"vue\";\nawait nextTick();\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_this_next_tick_callback_in_script() {
        let src = "<script>\nexport default {\n  mounted() {\n    this.$nextTick(() => {\n      updateDom();\n    });\n  },\n};\n</script>";
        assert_eq!(run(src).len(), 1);
    }
}
