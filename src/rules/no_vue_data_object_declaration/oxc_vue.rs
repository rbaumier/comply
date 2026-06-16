//! no-vue-data-object-declaration Vue SFC backend (oxc-based).
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

    // --- Biome `.vue` fixtures (invalid) ---

    #[test]
    fn flags_export_default_object_data() {
        let src = "<script>\nexport default {\n  data: {\n    foo: 'bar'\n  }\n};\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_export_default_parenthesized_object_data() {
        let src =
            "<script>\nexport default {\n  data: /*a*/ (/*b*/{\n    foo: 'bar'\n  })\n};\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_create_app_object_data() {
        let src = "<script>\nimport { createApp } from 'vue';\ncreateApp({\n  data: {\n    foo: 'bar'\n  }\n});\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_create_app_mount_object_data() {
        let src = "<script>\nimport { createApp } from 'vue';\ncreateApp({\n  data: {\n    foo: 'bar'\n  }\n}).mount('#app');\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    // --- Biome `.vue` fixtures (valid) ---

    #[test]
    fn allows_arrow_data() {
        let src = "<script>\nexport default {\n  data: () => {\n    // no-op\n  }\n};\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_function_data() {
        let src = "<script>\nexport default {\n  data: function () {\n    return { foo: 'bar' };\n  }\n};\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_method_data_with_methods() {
        let src = "<script>\nexport default {\n  data() { },\n  methods: {},\n};\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_method_data_in_methods_local() {
        // `data: {}` on a local object inside a method is not the component's
        // `data` option.
        let src = "<script>\nexport default {\n  methods: {\n    foo() {\n      const bar = { data: {} };\n    }\n  }\n};\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_create_app_spread_method_data() {
        let src = "<script>\nimport { createApp } from 'vue';\ncreateApp({\n  ...data,\n  data() {\n    return { foo: 'bar' };\n  }\n}).mount('#app');\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_app_component_function_data() {
        // `app.component('x', { … })` is not a detected component shape.
        let src = "<script>\nimport { createApp } from 'vue';\nconst app = createApp(App);\napp.component('some-comp', {\n  data: function () {\n    return { foo: 'bar' };\n  }\n});\n</script>";
        assert!(run(src).is_empty());
    }
}
