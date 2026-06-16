//! vue-valid-v-once AST backend.
//!
//! Walks `directive_attribute` nodes. For each one whose `directive_name` is
//! `v-once`, reports when the directive carries an argument, modifiers, or a
//! value. Only the bare `v-once` form is accepted.

use crate::diagnostic::{Diagnostic, Severity};

/// The kind of `v-once` violation, mapped to its diagnostic message.
enum Violation {
    Argument,
    Modifiers,
    Value,
}

impl Violation {
    fn message(&self) -> &'static str {
        match self {
            Self::Argument => "The v-once directive must not have an argument.",
            Self::Modifiers => "The v-once directive does not support modifiers.",
            Self::Value => "The v-once directive must not have a value.",
        }
    }
}

/// Read the `directive_name` text of a `directive_attribute` node.
fn directive_name<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = directive.walk();
    directive
        .children(&mut cursor)
        .find(|c| c.kind() == "directive_name")
        .and_then(|n| n.utf8_text(source).ok())
}

/// Classify a `v-once` `directive_attribute`, returning the first violation
/// found in Biome's check order (argument, modifier, value), or `None` when
/// the usage is the bare `v-once`.
fn classify(directive: tree_sitter::Node) -> Option<Violation> {
    let mut has_argument = false;
    let mut has_modifiers = false;
    let mut has_value = false;
    let mut cursor = directive.walk();
    for child in directive.children(&mut cursor) {
        match child.kind() {
            "directive_argument" | "directive_dynamic_argument" => has_argument = true,
            "directive_modifiers" => has_modifiers = true,
            "attribute_value" | "quoted_attribute_value" => has_value = true,
            _ => {}
        }
    }

    if has_argument {
        Some(Violation::Argument)
    } else if has_modifiers {
        Some(Violation::Modifiers)
    } else if has_value {
        Some(Violation::Value)
    } else {
        None
    }
}

crate::ast_check! { on ["directive_attribute"] prefilter = ["v-once"] => |node, source, ctx, diagnostics|
    if directive_name(node, source) != Some("v-once") {
        return;
    }
    let Some(violation) = classify(node) else {
        return;
    };
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: violation.message().into(),
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
    fn flags_argument() {
        let diags = run(&wrap("<div v-once:arg></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("must not have an argument"));
    }

    #[test]
    fn flags_modifier() {
        let diags = run(&wrap("<div v-once.mod></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not support modifiers"));
    }

    #[test]
    fn flags_value() {
        let diags = run(&wrap("<div v-once=\"value\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("must not have a value"));
    }

    #[test]
    fn flags_all_biome_invalid_fixtures() {
        let source = wrap(
            "<div v-once:arg></div>\n\
             <div v-once.mod></div>\n\
             <div v-once=\"value\"></div>",
        );
        assert_eq!(run(&source).len(), 3);
    }

    // --- Valid fixtures (Biome valid.vue) ---

    #[test]
    fn allows_bare_v_once() {
        assert!(run(&wrap("<div v-once></div>")).is_empty());
    }

    #[test]
    fn ignores_other_directives_with_value() {
        assert!(run(&wrap("<div v-text=\"foo\"></div>")).is_empty());
    }

    #[test]
    fn allows_bare_v_once_alongside_plain_attributes() {
        assert!(run(&wrap("<div id=\"x\" class=\"y\" v-once></div>")).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_other_directives() {
        assert!(run(&wrap("<div v-show=\"ok\" v-bind:id=\"x\"></div>")).is_empty());
    }
}
