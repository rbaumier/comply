//! vue-valid-v-text AST backend.
//!
//! Walks `directive_attribute` nodes. For each one whose `directive_name` is
//! `v-text`, reports when the directive carries an argument or modifiers, or
//! lacks a non-empty value (a bare `v-text`, `v-text=""`, or a whitespace-only
//! `v-text="   "`).

use crate::diagnostic::{Diagnostic, Severity};

/// The kind of `v-text` violation, mapped to its diagnostic message.
enum Violation {
    Argument,
    Modifiers,
    MissingValue,
}

impl Violation {
    fn message(&self) -> &'static str {
        match self {
            Self::Argument => "The v-text directive does not accept an argument.",
            Self::Modifiers => "The v-text directive does not support modifiers.",
            Self::MissingValue => "The v-text directive is missing a value.",
        }
    }
}

/// Find the first child of `node` whose kind is in `kinds`.
fn child_of_kind<'a>(node: tree_sitter::Node<'a>, kinds: &[&str]) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| kinds.contains(&c.kind()))
}

/// Read the `directive_name` text of a `directive_attribute` node.
fn directive_name<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    child_of_kind(directive, &["directive_name"]).and_then(|n| n.utf8_text(source).ok())
}

/// Read the value expression of a `directive_attribute`, descending through a
/// `quoted_attribute_value` wrapper. `None` when the directive has no value or
/// an empty quoted value (`v-text=""` yields no inner `attribute_value`).
fn directive_value<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = directive.walk();
    for child in directive.children(&mut cursor) {
        match child.kind() {
            "attribute_value" => return child.utf8_text(source).ok(),
            "quoted_attribute_value" => {
                return child_of_kind(child, &["attribute_value"])
                    .and_then(|n| n.utf8_text(source).ok());
            }
            _ => {}
        }
    }
    None
}

/// Classify a `v-text` `directive_attribute`, returning the first violation
/// found in Biome's check order, or `None` when the usage is valid.
fn classify(directive: tree_sitter::Node, source: &[u8]) -> Option<Violation> {
    if child_of_kind(directive, &["directive_argument", "directive_dynamic_argument"]).is_some() {
        return Some(Violation::Argument);
    }
    if child_of_kind(directive, &["directive_modifiers"]).is_some() {
        return Some(Violation::Modifiers);
    }
    match directive_value(directive, source) {
        Some(value) if !value.trim().is_empty() => None,
        _ => Some(Violation::MissingValue),
    }
}

crate::ast_check! { on ["directive_attribute"] prefilter = ["v-text"] => |node, source, ctx, diagnostics|
    if directive_name(node, source) != Some("v-text") {
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
    fn flags_bare_missing_value() {
        let diags = run(&wrap("<div v-text></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a value"));
    }

    #[test]
    fn flags_empty_value() {
        let diags = run(&wrap("<div v-text=\"\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a value"));
    }

    #[test]
    fn flags_whitespace_only_value() {
        let diags = run(&wrap("<div v-text=\"    \"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a value"));
    }

    #[test]
    fn flags_argument() {
        let diags = run(&wrap("<div v-text:aaa=\"foo\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not accept an argument"));
    }

    #[test]
    fn flags_modifier() {
        let diags = run(&wrap("<div v-text.bbb=\"foo\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not support modifiers"));
    }

    #[test]
    fn flags_argument_without_value() {
        let diags = run(&wrap("<div v-text:aaa></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not accept an argument"));
    }

    #[test]
    fn flags_modifier_without_value() {
        let diags = run(&wrap("<div v-text.bbb></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not support modifiers"));
    }

    #[test]
    fn flags_self_closing_forms() {
        assert_eq!(run(&wrap("<div v-text />")).len(), 1);
        assert_eq!(run(&wrap("<div v-text=\"\" />")).len(), 1);
        assert_eq!(run(&wrap("<div v-text:aaa=\"foo\" />")).len(), 1);
        assert_eq!(run(&wrap("<div v-text.bbb=\"foo\" />")).len(), 1);
    }

    #[test]
    fn flags_all_biome_invalid_fixtures() {
        // Mirrors Biome's invalid.vue — 11 diagnostics total.
        let source = wrap(
            "<div v-text></div>\n\
             <div v-text=\"\"></div>\n\
             <div v-text=\"    \"></div>\n\
             <div v-text:aaa=\"foo\"></div>\n\
             <div v-text.bbb=\"foo\"></div>\n\
             <div v-text:aaa></div>\n\
             <div v-text.bbb></div>\n\
             <div v-text />\n\
             <div v-text=\"\" />\n\
             <div v-text:aaa=\"foo\" />\n\
             <div v-text.bbb=\"foo\" />",
        );
        assert_eq!(run(&source).len(), 11);
    }

    // --- Valid fixtures (Biome valid.vue) ---

    #[test]
    fn allows_v_text_with_value() {
        assert!(run(&wrap("<div v-text=\"foo\"></div>")).is_empty());
    }

    #[test]
    fn allows_v_text_with_value_self_closing() {
        assert!(run(&wrap("<div v-text=\"foo\" />")).is_empty());
    }

    #[test]
    fn allows_all_biome_valid_fixtures() {
        let source = wrap(
            "<div v-text=\"foo\"></div>\n\
             <div v-text=\"foo\" />",
        );
        assert!(run(&source).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn allows_single_quoted_value() {
        assert!(run(&wrap("<div v-text='foo'></div>")).is_empty());
    }

    #[test]
    fn ignores_other_directives() {
        assert!(run(&wrap("<div v-html=\"foo\" v-bind:id=\"x\"></div>")).is_empty());
    }

    #[test]
    fn ignores_v_text_substring_directive() {
        // A directive name that merely starts with `v-text` must not match.
        assert!(run(&wrap("<div v-textarea=\"foo\"></div>")).is_empty());
    }
}
