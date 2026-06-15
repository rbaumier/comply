//! vue-valid-v-if AST backend.
//!
//! Walks `directive_attribute` nodes. For each one whose `directive_name` is
//! `v-if`, reports when the directive carries an argument or modifiers, lacks a
//! value, or shares its element with a `v-else`/`v-else-if` directive.

use crate::diagnostic::{Diagnostic, Severity};

/// The kind of `v-if` violation, mapped to its diagnostic message.
enum Violation {
    Argument,
    Modifiers,
    MissingValue,
    ConflictingElse,
}

impl Violation {
    fn message(&self) -> &'static str {
        match self {
            Self::Argument => "`v-if` cannot have an argument.",
            Self::Modifiers => "`v-if` cannot have modifiers.",
            Self::MissingValue => "`v-if` requires a value expression.",
            Self::ConflictingElse => {
                "`v-if` cannot be used on an element that also has `v-else` or `v-else-if`."
            }
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

/// Whether any sibling `directive_attribute` on the same element is a
/// `v-else` or `v-else-if`.
fn has_conflicting_else(directive: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(tag) = directive.parent() else {
        return false;
    };
    let mut cursor = tag.walk();
    tag.children(&mut cursor).any(|sibling| {
        sibling.kind() == "directive_attribute"
            && matches!(directive_name(sibling, source), Some("v-else" | "v-else-if"))
    })
}

/// Classify a `v-if` `directive_attribute`, returning the first violation
/// found in Biome's check order, or `None` when the usage is valid.
fn classify(directive: tree_sitter::Node, source: &[u8]) -> Option<Violation> {
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
    } else if !has_value {
        Some(Violation::MissingValue)
    } else if has_conflicting_else(directive, source) {
        Some(Violation::ConflictingElse)
    } else {
        None
    }
}

crate::ast_check! { on ["directive_attribute"] prefilter = ["v-if"] => |node, source, ctx, diagnostics|
    if directive_name(node, source) != Some("v-if") {
        return;
    }
    let Some(violation) = classify(node, source) else {
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
        let diags = run(&wrap("<div v-if:aaa=\"foo\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("cannot have an argument"));
    }

    #[test]
    fn flags_modifier() {
        let diags = run(&wrap("<div v-if.mod=\"foo\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("cannot have modifiers"));
    }

    #[test]
    fn flags_missing_value() {
        let diags = run(&wrap("<div v-if></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("requires a value"));
    }

    #[test]
    fn flags_v_if_with_v_else_same_element() {
        let diags = run(&wrap("<div v-if=\"foo\" v-else></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("v-else"));
    }

    #[test]
    fn flags_v_if_with_v_else_if_same_element() {
        let diags = run(&wrap("<div v-if=\"foo\" v-else-if=\"bar\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("v-else"));
    }

    #[test]
    fn flags_all_biome_invalid_fixtures() {
        let source = wrap(
            "<div v-if:aaa=\"foo\"></div>\n\
             <div v-if.mod=\"foo\"></div>\n\
             <div v-if></div>\n\
             <div v-if=\"foo\" v-else></div>\n\
             <div v-if=\"foo\" v-else-if=\"bar\"></div>",
        );
        assert_eq!(run(&source).len(), 5);
    }

    // --- Valid fixtures (Biome valid.vue) ---

    #[test]
    fn allows_simple_v_if() {
        assert!(run(&wrap("<div v-if=\"ok\"></div>")).is_empty());
    }

    #[test]
    fn allows_comparison_value() {
        assert!(run(&wrap("<div v-if=\"a < b\"></div>")).is_empty());
    }

    #[test]
    fn allows_v_if_else_if_else_chain_on_siblings() {
        let source = wrap(
            "<div v-if=\"a\"></div>\n\
             <div v-else-if=\"b\"></div>\n\
             <div v-else></div>",
        );
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_v_if_alongside_plain_attributes() {
        assert!(run(&wrap("<div id=\"x\" class=\"y\" v-if=\"flag\"></div>")).is_empty());
    }

    #[test]
    fn allows_nested_conditional_with_self_closing() {
        let source = wrap(
            "<div v-if=\"cond1\"></div>\n\
             <div v-else><span v-if=\"cond2\"/></div>",
        );
        assert!(run(&source).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_v_else_alone() {
        assert!(run(&wrap("<div v-else></div>")).is_empty());
    }

    #[test]
    fn ignores_other_directives() {
        assert!(run(&wrap("<div v-show=\"ok\" v-bind:id=\"x\"></div>")).is_empty());
    }

    #[test]
    fn allows_single_quoted_value() {
        assert!(run(&wrap("<div v-if='ok'></div>")).is_empty());
    }
}
