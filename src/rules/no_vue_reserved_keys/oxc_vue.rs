//! no-vue-reserved-keys Vue SFC backend (oxc-based).
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
    fn flags_data_object() {
        // invalid-data-object.vue: `$el` reserved, `_foo` reserved in data.
        let src =
            "<script>\nexport default {\n  data: {\n    $el: '',\n    _foo: String,\n  },\n};\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_data_function() {
        let src = "<script>\nexport default {\n  data: function() {\n    return { $el: '', _foo: String };\n  }\n};\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_data_arrow_function() {
        let src = "<script>\nexport default {\n  data: () => {\n    return { $el: '', _foo: String };\n  }\n};\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_data_short_arrow_function() {
        let src = "<script>\nexport default {\n  data: () => ({ $el: '', _foo: String })\n};\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_data_method() {
        let src = "<script>\nexport default {\n  data() {\n    return { $el: '', _foo: String };\n  }\n};\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_async_data() {
        let src = "<script>\nexport default {\n  asyncData() {\n    return { $el: '', _foo: String };\n  }\n};\n</script>";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_computed() {
        let src = "<script>\nexport default {\n  computed: {\n    $el() {},\n    $data: () => {},\n    $props: () => ({}),\n    $options: function() {},\n    $children: { get() {}, set(value) {} }\n  }\n};\n</script>";
        assert_eq!(run(src).len(), 5);
    }

    #[test]
    fn flags_methods() {
        let src = "<script>\nexport default {\n  methods: {\n    $el() {},\n    $data: () => {},\n    $props: () => ({}),\n    $options: function() {},\n  }\n};\n</script>";
        assert_eq!(run(src).len(), 4);
    }

    #[test]
    fn flags_props_array() {
        let src = "<script>\nexport default {\n  props: ['$el'],\n};\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_props_object() {
        let src = "<script>\nexport default {\n  props: {\n    $el: String,\n  }\n};\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_define_props_object() {
        let src = "<script setup>\ndefineProps({\n  $el: String\n});\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_define_props_type_inline() {
        let src = "<script setup lang=\"ts\">\ndefineProps<{$el: string}>();\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_define_props_interface() {
        let src = "<script setup lang=\"ts\">\ninterface Props {\n  $el: string\n}\ndefineProps<Props>();\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_define_props_type_alias() {
        let src = "<script setup lang=\"ts\">\ntype A = {\n  $el: string\n};\ndefineProps<A>();\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    // --- Biome `.vue` fixtures (valid) ---

    #[test]
    fn allows_plain_data() {
        let src = "<script>\nexport default {\n  data() {\n    return { message: 'Hello Vue!', count: 0 };\n  }\n};\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_plain_props_object() {
        let src = "<script>\nexport default {\n  props: {\n    foo: String,\n  }\n};\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_underscore_outside_data() {
        // valid-setup-data.vue: `_foo` in methods and `_bar` in setup return are
        // fine — the `_` prefix is reserved only in `data`.
        let src = "<script>\nexport default {\n  props: ['foo'],\n  computed: { bar() {} },\n  data: () => ({ dat: null }),\n  methods: { _foo () {}, test () {} },\n  setup() {\n    return { _bar: () => {} };\n  }\n};\n</script>";
        assert!(run(src).is_empty());
    }
}
