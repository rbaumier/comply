//! no-vue-v-on-number-values AST backend.
//!
//! Walks `directive_attribute` nodes that are `v-on` bindings — the longhand
//! `v-on` `directive_name` or the `@` shorthand. Reports the directive when any
//! `directive_modifier` is entirely ASCII digits (a deprecated `keyCode`).

use crate::diagnostic::{Diagnostic, Severity};

/// Read the `directive_name` text of a `directive_attribute` node.
fn directive_name<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = directive.walk();
    directive
        .children(&mut cursor)
        .find(|c| c.kind() == "directive_name")
        .and_then(|n| n.utf8_text(source).ok())
}

/// Whether this `directive_attribute` is a `v-on` binding: the longhand `v-on`
/// name or the `@` shorthand.
fn is_v_on(directive: tree_sitter::Node, source: &[u8]) -> bool {
    matches!(directive_name(directive, source), Some("v-on" | "@"))
}

/// Whether the directive carries an all-digit (`keyCode`) modifier.
fn has_number_modifier(directive: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = directive.walk();
    let Some(modifiers) = directive
        .children(&mut cursor)
        .find(|c| c.kind() == "directive_modifiers")
    else {
        return false;
    };
    let mut mod_cursor = modifiers.walk();
    modifiers
        .children(&mut mod_cursor)
        .filter(|c| c.kind() == "directive_modifier")
        .any(|m| match m.utf8_text(source) {
            Ok(text) => !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()),
            Err(_) => false,
        })
}

crate::ast_check! { on ["directive_attribute"] prefilter = ["v-on", "@"] => |node, source, ctx, diagnostics|
    if !is_v_on(node, source) || !has_number_modifier(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Number modifiers are deprecated on Vue `v-on` directives. Use a named key modifier (e.g. `@keyup.enter`) or handle the key code in the event handler.".into(),
        severity: Severity::Error,
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
    fn flags_longhand_keycode() {
        let diags = run(&wrap("<input v-on:keyup.13=\"submit\" />"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Number modifiers are deprecated"));
    }

    #[test]
    fn flags_shorthand_keyup_keycode() {
        assert_eq!(run(&wrap("<input @keyup.27=\"cancel\" />")).len(), 1);
    }

    #[test]
    fn flags_shorthand_keydown_keycode() {
        assert_eq!(run(&wrap("<input @keydown.27=\"cancel\" />")).len(), 1);
    }

    #[test]
    fn flags_keycode_alongside_named_modifier() {
        assert_eq!(run(&wrap("<input @keyup.13.exact=\"submit\" />")).len(), 1);
    }

    #[test]
    fn flags_all_biome_invalid_fixtures() {
        let source = wrap(
            "<input v-on:keyup.13=\"submit\" />\n\
             <input @keyup.27=\"cancel\" />\n\
             <input @keydown.27=\"cancel\" />\n\
             <input @keyup.13.exact=\"submit\" />",
        );
        assert_eq!(run(&source).len(), 4);
    }

    // --- Valid fixtures (Biome valid.vue) ---

    #[test]
    fn allows_named_key_modifier_longhand() {
        assert!(run(&wrap("<input v-on:keyup.enter=\"submit\" />")).is_empty());
    }

    #[test]
    fn allows_named_key_modifier_shorthand() {
        assert!(run(&wrap("<input @keyup.esc=\"cancel\" />")).is_empty());
    }

    #[test]
    fn allows_named_event_modifier() {
        assert!(run(&wrap("<input @click.once=\"submit\" />")).is_empty());
    }

    #[test]
    fn allows_v_bind() {
        assert!(run(&wrap("<input v-bind:value=\"value\" />")).is_empty());
    }

    #[test]
    fn allows_all_biome_valid_fixtures() {
        let source = wrap(
            "<input v-on:keyup.enter=\"submit\" />\n\
             <input @keyup.esc=\"cancel\" />\n\
             <input @click.once=\"submit\" />\n\
             <input v-bind:value=\"value\" />",
        );
        assert!(run(&source).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_v_on_without_modifier() {
        assert!(run(&wrap("<input @keyup=\"submit\" />")).is_empty());
        assert!(run(&wrap("<button v-on=\"handlers\">go</button>")).is_empty());
    }

    #[test]
    fn ignores_number_modifier_on_other_directives() {
        // A numeric modifier on a non-`v-on` directive is out of scope.
        assert!(run(&wrap("<input v-bind:foo.13=\"x\" />")).is_empty());
    }

    #[test]
    fn ignores_static_attribute() {
        assert!(run(&wrap("<input value=\"13\" />")).is_empty());
    }
}
