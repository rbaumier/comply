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

/// The previous sibling element of `element` whose conditional chain status can
/// be evaluated, skipping `text`, `comment`, and other non-element nodes.
///
/// `element` and `template_element` are real elements and stop the walk: the one
/// nearest before `element` is its definitive predecessor in the chain. The Vue
/// grammar parses `<component>` (with an `:is`/`v-if`/`is` attribute) as a
/// `vue_component` or, when self-closing, as an `ERROR` node, so such a node is
/// returned only when it carries a `v-if`/`v-else-if` directive; otherwise it is
/// a parse artifact (e.g. a complex `:is` ternary or an object spread) and the
/// walk continues to the real preceding element rather than treating the artifact
/// as the predecessor.
fn previous_element_sibling<'a>(
    element: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut sibling = element.prev_sibling();
    while let Some(node) = sibling {
        match node.kind() {
            "element" | "template_element" => return Some(node),
            "vue_component" | "ERROR" if component_text_has_conditional(node, source) => {
                return Some(node);
            }
            _ => {}
        }
        sibling = node.prev_sibling();
    }
    None
}

/// Whether the source text of a `<component>` node carries a `v-if`/`v-else-if`
/// directive. The Vue grammar parses `<component>` as a `vue_component`/`ERROR`
/// node and mangles its directives instead of emitting clean
/// `directive_attribute` children, so the chain link is detected from the raw
/// text rather than the AST.
fn component_text_has_conditional(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.utf8_text(source)
        .map(text_has_conditional_directive)
        .unwrap_or(false)
}

/// Whether `text` (a start tag or `<component>` node source) carries a standalone
/// `v-if`/`v-else-if` directive rather than the substring appearing inside
/// another attribute name.
fn text_has_conditional_directive(text: &str) -> bool {
    text.match_indices("v-if")
        .chain(text.match_indices("v-else-if"))
        .any(|(start, pat)| {
            let before_is_boundary = source_is_directive_boundary(text.as_bytes(), start);
            let after = text.as_bytes().get(start + pat.len());
            let after_is_boundary =
                matches!(after, None | Some(b'=' | b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>'));
            before_is_boundary && after_is_boundary
        })
}

/// Whether the byte before `start` delimits the start of an attribute (so the
/// match is a standalone directive, not a substring of another attribute name).
fn source_is_directive_boundary(bytes: &[u8], start: usize) -> bool {
    match start.checked_sub(1).map(|i| bytes[i]) {
        None => true,
        Some(b) => b.is_ascii_whitespace() || b == b'<',
    }
}

/// Whether the previous sibling element carries a valid `v-if`/`v-else-if`
/// directive, forming a valid conditional chain.
///
/// When the AST lookup fails and `element`'s own subtree contains a built-in
/// `<component>` — whose directive-carrying form the Vue grammar mangles into a
/// `vue_component`/`ERROR` node that corrupts tree-sitter's sibling links — the
/// predecessor is recovered from the raw source instead. A genuinely orphaned
/// `v-else-if` still finds no preceding conditional sibling and is reported.
fn has_previous_conditional(directive: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(element) = owning_element(directive) else {
        return false;
    };
    if ast_has_previous_conditional(element, source) {
        return true;
    }
    if !subtree_has_component(element, source) {
        return false;
    }
    preceding_sibling_start_tag_text(element, source).is_some_and(text_has_conditional_directive)
}

/// Whether the previous sibling element found through tree-sitter carries a valid
/// `v-if`/`v-else-if` directive.
fn ast_has_previous_conditional(element: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(previous) = previous_element_sibling(element, source) else {
        return false;
    };
    if matches!(previous.kind(), "vue_component" | "ERROR") {
        // Only conditional-carrying `vue_component`/`ERROR` nodes are returned, so
        // reaching one here means the chain link is present.
        return true;
    }
    start_tag_directives(previous).any(|dir| is_valid_chain_directive(dir, source))
}

/// Whether `element`'s own source text nests a built-in `<component>` tag. Its
/// directive-carrying form is the structural trigger for the tree-sitter sibling
/// corruption that `preceding_sibling_start_tag_text` works around.
fn subtree_has_component(element: tree_sitter::Node, source: &[u8]) -> bool {
    element
        .utf8_text(source)
        .map(|text| text.contains("<component"))
        .unwrap_or(false)
}

/// The start-tag text of `element`'s nearest preceding sibling, recovered by
/// tokenizing the raw source before it. Tags are scanned left-to-right while a
/// stack tracks nesting; the last element that opens and closes at `element`'s own
/// depth is its preceding sibling. Returns `None` when `element` is the first
/// child (no preceding sibling exists).
fn preceding_sibling_start_tag_text<'a>(
    element: tree_sitter::Node,
    source: &'a [u8],
) -> Option<&'a str> {
    let prefix = source.get(..element.start_byte())?;
    // Each stack entry is an open ancestor tag and records that ancestor's most
    // recently completed child (its start-tag byte range); the base entry covers
    // the document level. `open_tags` holds the start-tag range of every open
    // ancestor, parallel to `stack[1..]`.
    let mut stack: Vec<Option<(usize, usize)>> = vec![None];
    let mut open_tags: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < prefix.len() {
        if prefix[i] != b'<' {
            i += 1;
            continue;
        }
        match prefix.get(i + 1) {
            Some(b'!') => i = skip_markup_declaration(prefix, i),
            Some(b'/') => {
                let end = tag_end(prefix, i);
                if let Some(range) = open_tags.pop() {
                    stack.pop();
                    if let Some(frame) = stack.last_mut() {
                        *frame = Some(range);
                    }
                }
                i = end;
            }
            Some(c) if c.is_ascii_alphabetic() => {
                let end = tag_end(prefix, i);
                let self_closing = prefix.get(end.saturating_sub(2)..end) == Some(b"/>".as_slice())
                    || is_void_element(prefix, i);
                if self_closing {
                    if let Some(frame) = stack.last_mut() {
                        *frame = Some((i, end));
                    }
                } else {
                    open_tags.push((i, end));
                    stack.push(None);
                }
                i = end;
            }
            // A `<` that is not a tag start (e.g. inside a `a < b` expression).
            _ => i += 1,
        }
    }
    let (start, end) = (*stack.last()?)?;
    std::str::from_utf8(source.get(start..end)?).ok()
}

/// The byte index just past the `>` closing the tag opening at `open`, skipping
/// quoted attribute values so a `>` inside a value does not end the tag early.
fn tag_end(bytes: &[u8], open: usize) -> usize {
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'"' | b'\'') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                i += 1;
            }
            b'>' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// The byte index just past a `<!-- -->` comment or `<! ... >` declaration
/// opening at `open`.
fn skip_markup_declaration(bytes: &[u8], open: usize) -> usize {
    if bytes[open..].starts_with(b"<!--") {
        let mut i = open + 4;
        while i < bytes.len() {
            if bytes[i..].starts_with(b"-->") {
                return i + 3;
            }
            i += 1;
        }
        return bytes.len();
    }
    tag_end(bytes, open)
}

/// Whether the tag opening at `open` names an HTML void element, which completes
/// without a matching end tag or a `/>` terminator.
fn is_void_element(bytes: &[u8], open: usize) -> bool {
    let name_start = open + 1;
    let mut end = name_start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'-') {
        end += 1;
    }
    let name = &bytes[name_start..end];
    const VOID: &[&[u8]] = &[
        b"area", b"base", b"br", b"col", b"embed", b"hr", b"img", b"input", b"link", b"meta",
        b"param", b"source", b"track", b"wbr",
    ];
    VOID.iter().any(|&candidate| name.eq_ignore_ascii_case(candidate))
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

    #[test]
    fn allows_v_else_if_after_component_with_v_if() {
        let diags = run(&wrap(
            "<component v-if=\"c1\" :is=\"comp\" /><span v-else-if=\"c2\">x</span>",
        ));
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_v_else_if_value_after_component_with_v_if() {
        let diags = run(&wrap(
            "<component v-if=\"a\" :is=\"x\" /><div v-else-if=\"b\" />",
        ));
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn flags_v_else_if_after_component_without_v_if() {
        // The preceding `<component>` carries no `v-if`, so the chain is invalid.
        let diags = run(&wrap(
            "<component :is=\"x\" /><span v-else-if=\"c2\">y</span>",
        ));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("must follow"));
    }

    #[test]
    fn allows_chain_when_v_else_has_object_spread_attribute() {
        // Regression for #5032: the `<Panel v-else>` carries a bound object
        // attribute containing a spread; the spread must not corrupt the
        // `v-if -> v-else-if` sibling tracking.
        let source = wrap(
            "<slot v-if=\"loading\" name=\"loader\"><Spin /></slot>\n\
             <slot v-else-if=\"isEmpty\" name=\"empty\">\n\
             <component :is=\"TreeSelectEmpty ? TreeSelectEmpty : 'Empty'\" />\n\
             </slot>\n\
             <Panel v-else :tree-props=\"{ blockNode: true, ...treeProps, data }\" @change=\"onSelectChange\" />",
        );
        let diags = run(&source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn malformed_component_artifact_between_chain_links_does_not_break_chain() {
        // A `<component :is="ternary" />` that the grammar emits as an ERROR
        // node sits between a valid `v-if` and the `v-else-if`. The artifact
        // must not be mistaken for the predecessor: the chain is still valid.
        let source = wrap(
            "<slot v-if=\"a\" />\n\
             <component :is=\"x ? a : 'B'\" />\n\
             <slot v-else-if=\"b\" />",
        );
        let diags = run(&source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn still_flags_orphan_v_else_if_after_only_a_malformed_component() {
        // When the ONLY preceding sibling is a non-conditional component
        // artifact and there is no real `v-if`, the `v-else-if` is orphaned.
        let source = wrap(
            "<component :is=\"x ? a : 'B'\" />\n\
             <slot v-else-if=\"b\" />",
        );
        let diags = run(&source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("must follow"));
    }

    #[test]
    fn allows_v_else_if_when_body_nests_component_directive() {
        // Regression for #7176: the flagged `<template v-else-if="c">` follows a
        // valid `v-if` sibling, but its own body nests a `<component v-if :is />`
        // whose `vue_component`/`ERROR` node corrupts tree-sitter sibling links.
        // The predecessor is recovered from source, so the chain is valid.
        let source = wrap(
            "<div><span v-if=\"a\">1</span>\
             <template v-else-if=\"c\"><component v-if=\"d\" :is=\"e\" /></template>\
             <template v-else-if=\"l\">tail</template></div>",
        );
        let diags = run(&source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn still_flags_orphan_v_else_if_when_body_nests_component_directive() {
        // The nested `<component v-if :is />` corrupts sibling links, but there is
        // genuinely no preceding conditional sibling, so the source recovery finds
        // none and the `v-else-if` is still reported.
        let source = wrap("<template v-else-if=\"x\"><component v-if=\"y\" :is=\"z\" /></template>");
        let diags = run(&source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("must follow"));
    }

    #[test]
    fn allows_chain_when_body_nests_plain_element_directive() {
        // Same shape as the #7176 fixture but nesting a plain `<span v-if />`
        // rather than a `<component>`: no parse corruption, so the AST path stays
        // authoritative and the fallback never engages.
        let source = wrap(
            "<div><span v-if=\"a\">1</span>\
             <template v-else-if=\"c\"><span v-if=\"d\" /></template>\
             <template v-else-if=\"l\">tail</template></div>",
        );
        let diags = run(&source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }
}
