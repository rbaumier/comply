//! use-vue-valid-v-else-if AST backend.
//!
//! Walks `directive_attribute` nodes. For each one whose `directive_name` is
//! `v-else-if`, reports when the directive carries an argument or modifiers,
//! lacks a value, shares its element with a `v-if`/`v-else` directive, or sits on
//! an element whose preceding sibling element does not carry a valid `v-if`/
//! `v-else-if` directive.

use crate::diagnostic::{Diagnostic, Severity};

/// The kind of `v-else-if` violation, mapped to its diagnostic message.
enum Violation {
    Argument,
    Modifiers,
    MissingValue,
    ConflictingDirective,
    MissingPreviousConditional,
}

impl Violation {
    fn message(&self) -> &'static str {
        match self {
            Self::Argument => "`v-else-if` cannot have an argument.",
            Self::Modifiers => "`v-else-if` cannot have modifiers.",
            Self::MissingValue => "`v-else-if` requires a value expression.",
            Self::ConflictingDirective => {
                "`v-else-if` cannot be used on an element that also has `v-if` or `v-else`."
            }
            Self::MissingPreviousConditional => {
                "`v-else-if` must follow an element that has `v-if` or `v-else-if`."
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

/// Whether a `directive_attribute` carries an argument, modifiers, or a value.
struct DirectiveShape {
    has_argument: bool,
    has_modifiers: bool,
    has_value: bool,
}

fn directive_shape(directive: tree_sitter::Node) -> DirectiveShape {
    let mut shape = DirectiveShape {
        has_argument: false,
        has_modifiers: false,
        has_value: false,
    };
    let mut cursor = directive.walk();
    for child in directive.children(&mut cursor) {
        match child.kind() {
            "directive_argument" | "directive_dynamic_argument" => shape.has_argument = true,
            "directive_modifiers" => shape.has_modifiers = true,
            "attribute_value" | "quoted_attribute_value" => shape.has_value = true,
            _ => {}
        }
    }
    shape
}

/// Whether the `directive_attribute` is a valid chain link: a bare `v-if`/
/// `v-else-if` with a value, no argument, and no modifiers. Used to validate the
/// preceding sibling element.
fn is_valid_chain_directive(directive: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(name) = directive_name(directive, source) else {
        return false;
    };
    if name != "v-if" && name != "v-else-if" {
        return false;
    }
    let shape = directive_shape(directive);
    !shape.has_argument && !shape.has_modifiers && shape.has_value
}

/// The `element` node that owns this directive (directive -> start_tag/
/// self_closing_tag -> element).
fn owning_element(directive: tree_sitter::Node) -> Option<tree_sitter::Node> {
    directive.parent().and_then(|tag| tag.parent())
}

/// Iterate the `directive_attribute` children of an element's opening tag
/// (`start_tag` for normal elements, `self_closing_tag` for self-closing ones).
fn start_tag_directives<'a>(
    element: tree_sitter::Node<'a>,
) -> impl Iterator<Item = tree_sitter::Node<'a>> {
    let mut cursor = element.walk();
    let opening_tag = element
        .children(&mut cursor)
        .find(|c| matches!(c.kind(), "start_tag" | "self_closing_tag"));
    opening_tag.into_iter().flat_map(|tag| {
        let mut cursor = tag.walk();
        tag.children(&mut cursor)
            .filter(|c| c.kind() == "directive_attribute")
            .collect::<Vec<_>>()
    })
}

/// Whether the directive shares its element with a `v-if` or `v-else` sibling
/// directive.
fn conflicts_with_other_directive(directive: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(element) = owning_element(directive) else {
        return false;
    };
    start_tag_directives(element).any(|sibling| {
        sibling.id() != directive.id()
            && matches!(directive_name(sibling, source), Some("v-if" | "v-else"))
    })
}

/// The previous sibling `element` of `element`, skipping `text`, `comment`, and
/// other non-element nodes.
fn previous_element_sibling(element: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut sibling = element.prev_sibling();
    while let Some(node) = sibling {
        if node.kind() == "element" || node.kind() == "template_element" {
            return Some(node);
        }
        sibling = node.prev_sibling();
    }
    None
}

/// Whether the previous sibling element carries a valid `v-if`/`v-else-if`
/// directive, forming a valid conditional chain.
fn has_previous_conditional(directive: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(element) = owning_element(directive) else {
        return false;
    };
    let Some(previous) = previous_element_sibling(element) else {
        return false;
    };
    start_tag_directives(previous).any(|dir| is_valid_chain_directive(dir, source))
}

/// Classify a `v-else-if` `directive_attribute`, returning the first violation
/// found, or `None` when the usage is valid.
fn classify(directive: tree_sitter::Node, source: &[u8]) -> Option<Violation> {
    let shape = directive_shape(directive);
    if shape.has_argument {
        Some(Violation::Argument)
    } else if shape.has_modifiers {
        Some(Violation::Modifiers)
    } else if !shape.has_value {
        Some(Violation::MissingValue)
    } else if conflicts_with_other_directive(directive, source) {
        Some(Violation::ConflictingDirective)
    } else if !has_previous_conditional(directive, source) {
        Some(Violation::MissingPreviousConditional)
    } else {
        None
    }
}

crate::ast_check! { on ["directive_attribute"] prefilter = ["v-else-if"] => |node, source, ctx, diagnostics|
    if directive_name(node, source) != Some("v-else-if") {
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
        let diags = run(&wrap(
            "<div v-if=\"a\"></div><div v-else-if:arg=\"b\"></div>",
        ));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("cannot have an argument"));
    }

    #[test]
    fn flags_dynamic_argument() {
        let diags = run(&wrap(
            "<div v-if=\"a\"></div><div v-else-if:[complex]=\"b\"></div>",
        ));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("cannot have an argument"));
    }

    #[test]
    fn flags_modifier() {
        let diags = run(&wrap(
            "<div v-if=\"a\"></div><div v-else-if.mod=\"b\"></div>",
        ));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("cannot have modifiers"));
    }

    #[test]
    fn flags_multiple_modifiers() {
        let diags = run(&wrap(
            "<div v-if=\"a\"></div><div v-else-if.mod1.mod2=\"b\"></div>",
        ));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("cannot have modifiers"));
    }

    #[test]
    fn flags_missing_value() {
        let diags = run(&wrap("<div v-if=\"a\"></div><div v-else-if></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("requires a value"));
    }

    #[test]
    fn flags_dangling_without_previous() {
        let diags = run(&wrap("<div v-else-if=\"b\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("must follow"));
    }

    #[test]
    fn flags_conflict_with_v_if_same_element() {
        let diags = run(&wrap("<div v-if=\"a\" v-else-if=\"b\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("v-if"));
    }

    #[test]
    fn flags_conflict_with_v_else_same_element() {
        let diags = run(&wrap("<div v-else v-else-if=\"b\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("v-else"));
    }

    #[test]
    fn flags_preceded_by_unrelated_element() {
        let diags = run(&wrap("<span></span><div v-else-if=\"b\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("must follow"));
    }

    #[test]
    fn flags_preceded_by_comment_only() {
        let diags = run(&wrap("<!-- comment --><div v-else-if=\"b\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("must follow"));
    }

    // --- Valid fixtures (Biome valid.vue) ---

    #[test]
    fn allows_v_if_then_v_else_if() {
        assert!(
            run(&wrap("<div v-if=\"a\"></div><div v-else-if=\"b\"></div>")).is_empty()
        );
    }

    #[test]
    fn allows_full_chain() {
        let source = wrap(
            "<div v-if=\"a\"></div><div v-else-if=\"b\"></div><div v-else-if=\"c\"></div><div v-else></div>",
        );
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_chain_with_interspersed_comments() {
        let source = wrap(
            "<div v-if=\"a\"></div>\n\
             <!-- comment -->\n\
             <div v-else-if=\"b\"></div>\n\
             <!-- another comment -->\n\
             <div v-else-if=\"c\"></div><div v-else></div>",
        );
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_complex_expression_chain() {
        let source = wrap(
            "<div v-if=\"user && user.isAdmin\"></div><div v-else-if=\"user && user.isModerator\"></div>",
        );
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_multiline_preceding_element() {
        let source = wrap(
            "<div\n\
             v-if=\"cond1\"\n\
             data-attr1\n\
             data-attr2\n\
             />\n\
             <div v-else-if=\"cond2\"></div>",
        );
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_template_element_chain() {
        let source = wrap(
            "<template v-if=\"condition\"><p>Content</p></template>\n\
             <template v-else-if=\"otherCondition\"><span>Other</span></template>",
        );
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_chain_after_unrelated_block() {
        let source = wrap(
            "<div v-if=\"a\"></div><div v-else-if=\"b\"></div><div v-else></div>\n\
             <span>Unrelated content</span>\n\
             <div v-if=\"x\"></div><div v-else-if=\"y\"></div><div v-else></div>",
        );
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_nested_conditional_with_self_closing() {
        let source = wrap(
            "<div v-if=\"cond1\"></div>\n\
             <div v-else-if=\"cond2\"><span v-if=\"cond3\" /></div>",
        );
        assert!(run(&source).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_v_if_alone() {
        assert!(run(&wrap("<div v-if=\"a\"></div>")).is_empty());
    }

    #[test]
    fn ignores_other_directives() {
        assert!(run(&wrap("<div v-show=\"ok\" v-bind:id=\"x\"></div>")).is_empty());
    }

    #[test]
    fn previous_element_with_invalid_v_if_does_not_count() {
        // The preceding element's `v-if` has no value, so it is not a valid
        // chain link and the `v-else-if` is still dangling.
        let diags = run(&wrap("<div v-if></div><div v-else-if=\"b\"></div>"));
        assert!(
            diags.iter().any(|d| d.message.contains("must follow")),
            "expected dangling diagnostic, got {diags:?}"
        );
    }
}
