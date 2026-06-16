//! vue-prefer-shorthand-v-on AST backend.
//!
//! Walks `directive_attribute` nodes. Flags a longhand `v-on:` binding that
//! carries an argument (`v-on:click`, `v-on:[dyn]`) since it has the `@click`
//! shorthand equivalent. An argument-less `v-on="obj"` has no shorthand and is
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

/// Whether the directive carries an argument (`:click` or `:[dyn]`).
fn has_argument(directive: tree_sitter::Node) -> bool {
    let mut cursor = directive.walk();
    directive
        .children(&mut cursor)
        .any(|c| matches!(c.kind(), "directive_argument" | "directive_dynamic_argument"))
}

crate::ast_check! { on ["directive_attribute"] prefilter = ["v-on"] => |node, source, ctx, diagnostics|
    if directive_name(node, source) != Some("v-on") || !has_argument(node) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Use the `@` shorthand instead of longhand `v-on:`.".into(),
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
    fn flags_longhand_v_on() {
        let diags = run(&wrap("<div v-on:click=\"onClick\" />"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("shorthand"));
    }

    #[test]
    fn flags_longhand_v_on_alongside_static_attr() {
        let diags = run(&wrap("<div disabled v-on:click=\"onClick\" />"));
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_all_biome_invalid_fixtures() {
        let source = wrap(
            "<div v-on:click=\"onClick\" />\n\
             <div disabled v-on:click=\"onClick\" />",
        );
        assert_eq!(run(&source).len(), 2);
    }

    #[test]
    fn flags_longhand_dynamic_argument() {
        assert_eq!(run(&wrap("<div v-on:[event]=\"onClick\" />")).len(), 1);
    }

    #[test]
    fn flags_longhand_with_modifier() {
        assert_eq!(run(&wrap("<div v-on:click.stop=\"onClick\" />")).len(), 1);
    }

    // --- Valid fixtures (Biome valid.vue) ---

    #[test]
    fn allows_shorthand_event_binding() {
        assert!(run(&wrap("<div @click=\"onClick\" />")).is_empty());
    }

    #[test]
    fn allows_shorthand_with_modifier() {
        assert!(run(&wrap("<div @click.stop=\"onClick\" />")).is_empty());
    }

    #[test]
    fn allows_argumentless_v_on() {
        assert!(run(&wrap("<button v-on=\"handlers\">go</button>")).is_empty());
    }

    #[test]
    fn allows_shorthand_dynamic_argument() {
        assert!(run(&wrap("<div @[event]=\"onClick\" />")).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_static_attribute() {
        assert!(run(&wrap("<div onclick=\"foo\" />")).is_empty());
    }

    #[test]
    fn ignores_other_directives() {
        assert!(run(&wrap("<div v-if=\"ok\" v-bind:foo=\"bar\" v-for=\"x in xs\" />")).is_empty());
    }

    #[test]
    fn ignores_v_model_and_v_slot() {
        assert!(run(&wrap("<input v-model=\"name\" />")).is_empty());
    }
}
