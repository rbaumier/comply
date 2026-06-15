//! vue-prefer-shorthand-v-bind AST backend.
//!
//! Walks `directive_attribute` nodes. Flags a longhand `v-bind:` binding that
//! carries an argument (`v-bind:foo`, `v-bind:[dyn]`) since it has the `:foo`
//! shorthand equivalent. An argument-less `v-bind="obj"` has no shorthand and is
//! left alone.

use crate::diagnostic::{Diagnostic, Severity};

/// Read the `directive_name` text of a `directive_attribute` node.
fn directive_name<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = directive.walk();
    directive
        .children(&mut cursor)
        .find(|c| c.kind() == "directive_name")
        .and_then(|n| n.utf8_text(source).ok())
}

/// Whether the directive carries an argument (`:foo` or `:[dyn]`).
fn has_argument(directive: tree_sitter::Node) -> bool {
    let mut cursor = directive.walk();
    directive
        .children(&mut cursor)
        .any(|c| matches!(c.kind(), "directive_argument" | "directive_dynamic_argument"))
}

crate::ast_check! { on ["directive_attribute"] prefilter = ["v-bind"] => |node, source, ctx, diagnostics|
    if directive_name(node, source) != Some("v-bind") || !has_argument(node) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Use the `:` shorthand instead of longhand `v-bind:`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    fn wrap(body: &str) -> String {
        format!("<template>\n{body}\n</template>")
    }

    // --- Invalid fixtures (Biome invalid.vue) ---

    #[test]
    fn flags_longhand_v_bind() {
        let diags = run(&wrap("<div v-bind:foo=\"bar\" />"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("shorthand"));
    }

    #[test]
    fn flags_longhand_v_bind_alongside_static_attr() {
        let diags = run(&wrap("<div disabled v-bind:foo=\"bar\" />"));
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_all_biome_invalid_fixtures() {
        let source = wrap(
            "<div v-bind:foo=\"bar\" />\n\
             <div disabled v-bind:foo=\"bar\" />",
        );
        assert_eq!(run(&source).len(), 2);
    }

    #[test]
    fn flags_longhand_dynamic_argument() {
        assert_eq!(run(&wrap("<div v-bind:[key]=\"bar\" />")).len(), 1);
    }

    #[test]
    fn flags_longhand_with_modifier() {
        assert_eq!(run(&wrap("<div v-bind:foo.sync=\"bar\" />")).len(), 1);
    }

    // --- Valid fixtures (Biome valid.vue) ---

    #[test]
    fn allows_shorthand_binding() {
        assert!(run(&wrap("<div :foo=\"bar\" />")).is_empty());
    }

    #[test]
    fn allows_argumentless_v_bind() {
        let source = wrap(
            "<DialogTrigger data-slot=\"dialog-trigger\" v-bind=\"props\">\n\
             <slot></slot>\n\
             </DialogTrigger>",
        );
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_shorthand_dynamic_argument() {
        assert!(run(&wrap("<div :[key]=\"bar\" />")).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_static_attribute() {
        assert!(run(&wrap("<div foo=\"bar\" />")).is_empty());
    }

    #[test]
    fn ignores_other_directives() {
        assert!(run(&wrap("<div v-if=\"ok\" v-on:click=\"go\" v-for=\"x in xs\" />")).is_empty());
    }

    #[test]
    fn ignores_v_model_and_v_slot() {
        assert!(run(&wrap("<input v-model=\"name\" />")).is_empty());
    }
}
