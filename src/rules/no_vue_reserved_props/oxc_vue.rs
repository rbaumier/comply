//! no-vue-reserved-props Vue SFC backend (oxc-based).
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

    // --- Biome `.vue` fixtures (invalid) — `ref` and `key` flagged, `foo` clean ---

    #[test]
    fn flags_export_default_object() {
        // invalid-export-default-object.vue
        let src = "<script>\nexport default {\n  props: {\n    ref: String,\n    key: String,\n    foo: String,\n  }\n};\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_export_default_array() {
        // invalid-export-default-array.vue
        let src = "<script>\nexport default {\n  props: [\n    'ref',\n    'key',\n    'foo',\n  ]\n};\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_create_app() {
        // invalid-create-app.vue
        let src = "<script>\nimport {createApp} from 'vue';\n\ncreateApp({\n  props: [\n    'ref',\n    'key',\n    'foo'\n  ]\n}).mount('#app');\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_define_props_object() {
        // invalid-define-props-object.vue
        let src = "<script setup>\ndefineProps({\n  ref: String,\n  key: String,\n  foo: String,\n});\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_define_props_array() {
        // invalid-define-props-array.vue
        let src = "<script setup>\ndefineProps(['ref', 'key', 'foo']);\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_define_props_type_inline() {
        // invalid-define-props-type-inline.vue
        let src = "<script setup lang=\"ts\">\ndefineProps<{\n  ref: string,\n  key: string,\n  foo: string,\n}>();\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_define_props_interface() {
        // invalid-define-props-interface.vue
        let src = "<script setup lang=\"ts\">\ninterface Props {\n  ref: string\n  key: string\n  foo: string\n}\ndefineProps<Props>();\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_define_props_type_alias() {
        // invalid-define-props-type-alias.vue
        let src = "<script setup lang=\"ts\">\ntype Props = {\n  ref: string\n  key: string\n  foo: string\n};\ndefineProps<Props>();\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    // --- Biome `.vue` fixtures (valid) ---

    #[test]
    fn allows_export_default_props_object() {
        // valid-export-default-props-object.vue
        let src = "<script>\nexport default {\n  props: {\n    foo: String,\n  }\n};\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_export_default_props_array() {
        // valid-export-default-props-array.vue
        let src = "<script>\nexport default {\n  props: ['foo'],\n};\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_export_default_non_props() {
        // valid-export-default-non-props.vue: `ref` in `data` is not a prop.
        let src = "<script>\nexport default {\n  data: {\n    ref: ''\n  }\n};\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_define_props_object() {
        // valid-define-props-object.vue
        let src = "<script setup>\ndefineProps({\n  foo: String,\n});\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_define_props_array() {
        // valid-define-props-array.vue
        let src = "<script setup>\ndefineProps(['foo']);\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_define_props_type_inline() {
        // valid-define-props-type-inline.vue
        let src = "<script setup lang=\"ts\">\ndefineProps<{\n  foo: string,\n}>();\n</script>";
        assert!(run(src).is_empty());
    }
}
