//! vue-typed-define-props-emits Vue SFC backend (oxc-based).
//!
//! Extracts `<script setup lang="ts">` blocks with tree-sitter-vue, then
//! delegates each one to `vue_sfc_oxc::run_oxc_check_on_vue_block`, which
//! re-parses the block with oxc and runs the shared `oxc_typescript::Check`.
//! Only `setup` blocks with `lang="ts"` are considered: the compiler macros
//! exist only in `<script setup>`, and the type form is required only in a
//! TypeScript SFC.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::{vue_sfc, vue_sfc_oxc};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let blocks = vue_sfc::extract_scripts(tree, ctx.source);
        let mut diagnostics = Vec::new();
        for block in blocks.iter().filter(|b| b.is_setup && b.lang == Some("ts")) {
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

    // --- Runtime spread composition is not flagged (#7509) ---

    #[test]
    fn allows_props_single_line_spread() {
        // jekip/naive-ui-admin repro: `defineProps({ ...basicProps })` spreads a
        // runtime props object; it has no type-only equivalent.
        let sfc = "<script setup lang=\"ts\">\nconst props = defineProps({ ...basicProps })\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_props_multiline_spread_with_members() {
        let sfc = "<script setup lang=\"ts\">\nconst props = defineProps({\n  ...basicProps,\n  title: { type: String },\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_emits_object_spread() {
        let sfc = "<script setup lang=\"ts\">\nconst emit = defineEmits({ ...inheritedEmits })\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_with_defaults_props_spread() {
        // `withDefaults(defineProps({ ...basicProps }), { ... })` — the inner
        // `defineProps` object spread still exempts the call.
        let sfc = "<script setup lang=\"ts\">\nconst props = withDefaults(defineProps({ ...basicProps }), { title: 'x' })\n</script>";
        assert!(run(sfc).is_empty());
    }

    // --- Plain runtime object/array forms still flag ---

    #[test]
    fn flags_runtime_props_object() {
        let sfc = "<script setup lang=\"ts\">\nconst p = defineProps({ msg: String })\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defineProps"));
    }

    #[test]
    fn flags_runtime_emits_array() {
        let sfc = "<script setup lang=\"ts\">\nconst e = defineEmits(['change'])\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defineEmits"));
    }

    #[test]
    fn flags_runtime_emits_object_no_spread() {
        let sfc = "<script setup lang=\"ts\">\nconst e = defineEmits({ change: null })\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_emits_array_even_with_spread() {
        // Only an OBJECT-literal spread exempts the call; an array form is always
        // the runtime form (matching the issue's scope).
        let sfc = "<script setup lang=\"ts\">\nconst e = defineEmits([...common, 'change'])\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    // --- Type form and non-ts SFCs are clean ---

    #[test]
    fn allows_type_form() {
        let sfc = "<script setup lang=\"ts\">\nconst p = defineProps<{ msg: string }>()\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn ignores_non_ts_setup() {
        let sfc = "<script setup>\nconst p = defineProps({ msg: String })\n</script>";
        assert!(run(sfc).is_empty());
    }

    // --- AST backend removes the text-scan's string/comment collateral ---

    #[test]
    fn ignores_define_props_inside_string() {
        // A line-based scan would flag `defineProps({` here; the AST sees only a
        // string literal.
        let sfc = "<script setup lang=\"ts\">\nconst doc = \"defineProps({ msg: String })\"\nconst p = defineProps<{ msg: string }>()\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn ignores_define_props_inside_comment() {
        let sfc = "<script setup lang=\"ts\">\n// defineProps({ msg: String })\nconst p = defineProps<{ msg: string }>()\n</script>";
        assert!(run(sfc).is_empty());
    }
}
