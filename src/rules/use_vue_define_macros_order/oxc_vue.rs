//! use-vue-define-macros-order Vue SFC backend (oxc-based).
//!
//! Extracts `<script setup>` blocks with tree-sitter-vue, then delegates each
//! one to `vue_sfc_oxc::run_oxc_check_on_vue_block`, which re-parses the block
//! with oxc and runs the shared `oxc_typescript::Check`. Non-`setup` blocks are
//! skipped — the compiler macros only exist in `<script setup>`.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::{vue_sfc, vue_sfc_oxc};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let blocks = vue_sfc::extract_scripts(tree, ctx.source);
        let mut diagnostics = Vec::new();
        for block in blocks.iter().filter(|b| b.is_setup) {
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

    // --- Biome `.vue` fixtures (invalid) ---

    #[test]
    fn flags_define_props_after_define_emits() {
        // invalid.vue: defineEmits then defineProps — defineProps is lower-order.
        let src = "<script lang=\"ts\" setup>\nimport { ref } from 'vue'\n\ninterface Foo {}\n\ntype Bar = 1\n\nexport type FooBar = Foo & Bar\n\ndebugger\n\n// Define the props and emits\nconst emit = defineEmits([])\n/** Props */\ndefineProps({})\n\nconst count = ref(0)\n</script>\n\n<template><div /></template>";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defineProps"));
    }

    #[test]
    fn flags_define_model_after_define_emits() {
        // invalid-a.vue: defineEmits then defineModel; withDefaults(defineNotProps)
        // is non-macro. defineModel is the lowest-order, reported.
        let src = "<script lang=\"ts\" setup>\nimport { ref } from 'vue'\n\ninterface Foo {}\n\ntype Bar = 1\n\nexport type FooBar = Foo & Bar\n\ndebugger\n\nconst emit = defineEmits([])\ndefineModel()\n\nwithDefaults(defineNotProps({}), { a: 1 })\n\nconst count = ref(0)\n</script>\n\n<template><div /></template>";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defineModel"));
    }

    #[test]
    fn flags_macro_after_non_macro_assignment() {
        // invalid-b.vue: a non-macro `const count = ref(0)` precedes defineEmits.
        let src = "<script lang=\"ts\" setup>\nconst count = ref(0)\n\nconst emit = defineEmits([])/** Hello */\n</script>\n\n<template><div /></template>";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defineEmits"));
    }

    #[test]
    fn flags_macro_after_non_macro_bare_call() {
        // invalid-c.vue: a non-macro bare `ref(0)` precedes defineEmits.
        let src = "<script lang=\"ts\" setup>\nref(0)\n\ndefineEmits([])// hello\n</script>\n\n<template><div /></template>";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defineEmits"));
    }

    #[test]
    fn flags_single_statement_multiple_declarators() {
        // invalid-single-a.vue: one statement, emit/model/count/props declarators;
        // props is the last macro declarator and is reported.
        let src = "<script lang=\"ts\" setup>\nconst emit = defineEmits([]), model = defineModel(), count = ref(0), props = defineProps();\n</script>\n\n<template><div /></template>";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defineProps"));
    }

    #[test]
    fn flags_single_statement_emit_then_props() {
        // invalid-single.vue: `const emit = defineEmits([]), props = defineProps({})`.
        let src = "<script lang=\"ts\" setup>\nimport { ref } from 'vue'\n\ninterface Foo {}\n\ntype Bar = 1\n\nexport type FooBar = Foo & Bar\n\ndebugger\n\nconst emit = defineEmits([]), props = defineProps({})\n\nconst count = ref(0)\n</script>\n\n<template><div /></template>";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defineProps"));
    }

    #[test]
    fn flags_with_defaults_define_props() {
        // invalid-with-defaults.vue: withDefaults(defineProps(...)) unwraps to
        // defineProps, which is lower-order than the preceding defineEmits.
        let src = "<script lang=\"ts\" setup>\nimport { ref } from 'vue'\n\ninterface Foo {}\n\ntype Bar = 1\n\nexport type FooBar = Foo & Bar\n\ndebugger\n\nconst emit = defineEmits([])\nconst props = withDefaults(defineProps({}), { a: 1 })\n\nconst count = ref(0)\n</script>\n\n<template><div /></template>";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defineProps"));
    }

    // --- Biome `.vue` fixtures (valid) ---

    #[test]
    fn allows_props_then_emits() {
        // valid.vue: defineProps then defineEmits, after skippable statements.
        let src = "<script lang=\"ts\" setup>\nimport { ref } from 'vue'\n\ninterface Foo {}\n\ntype Bar = 1\n\nexport type FooBar = Foo & Bar\n\ndebugger\n\ndefineProps({})\nconst emit = defineEmits([])\n\nconst count = ref(0)\n</script>\n\n<template><div /></template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_setup_script_block() {
        // The macros only matter in `<script setup>`. A plain `<script>` with the
        // same out-of-order calls is not flagged.
        let src = "<script>\nconst emit = defineEmits([])\ndefineProps({})\n</script>\n\n<template><div /></template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_macros_in_order_without_prior_content() {
        // defineModel, defineProps, defineEmits already in order — clean.
        let src = "<script setup>\nconst model = defineModel()\nconst props = defineProps()\nconst emit = defineEmits([])\n</script>\n\n<template><div /></template>";
        assert!(run(src).is_empty());
    }
}
