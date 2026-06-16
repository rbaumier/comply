//! use-vue-valid-template-root AST backend.
//!
//! Finds the first root-level `<template>` element and enforces the
//! src/content invariant: a `src` template must be empty, a src-less template
//! must hold non-whitespace content.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const MSG_MUST_BE_EMPTY: &str =
    "The root `<template>` with a `src` attribute must be empty. Remove its inline content.";
const MSG_MUST_HAVE_CONTENT: &str =
    "The root `<template>` is empty. Add content, or use a `src` attribute to load it externally.";

/// Find the first child of `node` whose kind is in `kinds`.
fn child_of_kind<'a>(node: tree_sitter::Node<'a>, kinds: &[&str]) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| kinds.contains(&c.kind()))
}

/// The `tag_name` text of an element's `start_tag`.
fn tag_name<'a>(element: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let start_tag = child_of_kind(element, &["start_tag"])?;
    child_of_kind(start_tag, &["tag_name"]).and_then(|n| n.utf8_text(source).ok())
}

/// Whether the element's `start_tag` carries a static `src` attribute.
fn has_src_attribute(element: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(start_tag) = child_of_kind(element, &["start_tag"]) else {
        return false;
    };
    let mut cursor = start_tag.walk();
    start_tag.children(&mut cursor).any(|child| {
        child.kind() == "attribute"
            && child_of_kind(child, &["attribute_name"])
                .and_then(|n| n.utf8_text(source).ok())
                == Some("src")
    })
}

/// Whether the element holds any non-whitespace content: a child element,
/// comment, or text node whose trimmed value is non-empty. The `start_tag` and
/// `end_tag` delimiters are skipped; whitespace-only text counts as empty.
fn has_non_whitespace_content(element: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = element.walk();
    element.children(&mut cursor).any(|child| match child.kind() {
        "start_tag" | "end_tag" => false,
        "text" => child
            .utf8_text(source)
            .is_ok_and(|t| !t.trim().is_empty()),
        _ => true,
    })
}

/// The first root-level `<template>` element among the document root's
/// direct children, mirroring Biome's "first top-level template" selection.
fn first_root_template<'a>(
    root: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = root.walk();
    root.children(&mut cursor).find(|child| {
        child.kind() == "template_element" && tag_name(*child, source) == Some("template")
    })
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let Some(template) = first_root_template(tree.root_node(), source) else {
            return Vec::new();
        };

        let has_src = has_src_attribute(template, source);
        let has_content = has_non_whitespace_content(template, source);

        let message = if has_src {
            if has_content {
                MSG_MUST_BE_EMPTY
            } else {
                return Vec::new();
            }
        } else if has_content {
            return Vec::new();
        } else {
            MSG_MUST_HAVE_CONTENT
        };

        vec![Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &template,
            super::META.id,
            message.to_string(),
            Severity::Error,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    // --- Biome invalid.vue ---

    #[test]
    fn flags_src_template_with_content() {
        let diags = run("<template src=\"./foo.html\">content</template>");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("must be empty"));
    }

    #[test]
    fn flags_empty_template_without_src() {
        let diags = run("<template></template>");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("is empty"));
    }

    #[test]
    fn only_first_root_template_is_checked() {
        // Mirrors Biome invalid.vue: only the first top-level `<template>` is
        // reported. The trailing empty `<template></template>` is ignored.
        let source = "<!-- should generate diagnostics -->\n\
             <template src=\"./foo.html\">content</template>\n\
             <template></template>\n";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("must be empty"));
        assert_eq!(diags[0].line, 2);
    }

    // --- Biome valid.vue ---

    #[test]
    fn allows_template_with_content() {
        assert!(run("<template>content</template>").is_empty());
    }

    #[test]
    fn allows_empty_src_template() {
        assert!(run("<template src=\"./foo.html\"></template>").is_empty());
    }

    #[test]
    fn allows_all_biome_valid_fixtures() {
        // Mirrors Biome valid.vue: a content template followed by an empty
        // src template. Only the first root template is checked, and it is
        // valid.
        let source = "<!-- should not generate diagnostics -->\n\
             <template>content</template>\n\
             <template src=\"./foo.html\"></template>\n";
        assert!(run(source).is_empty());
    }

    // --- Over/under-firing guards ---

    #[test]
    fn allows_template_with_element_child() {
        assert!(run("<template>\n<div>hi</div>\n</template>").is_empty());
    }

    #[test]
    fn flags_whitespace_only_template_as_empty() {
        let diags = run("<template>   \n  </template>");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("is empty"));
    }

    #[test]
    fn ignores_files_without_a_root_template() {
        assert!(run("<script>const a = 1;</script>").is_empty());
    }

    #[test]
    fn does_not_descend_into_nested_template() {
        // A nested `<template>` inside the root one is not the root; only the
        // root template's own content matters. Here the root has content.
        assert!(run("<template>\n<template></template>\n</template>").is_empty());
    }
}
