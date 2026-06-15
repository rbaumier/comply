//! vue-valid-v-for AST backend.
//!
//! Walks `directive_attribute` nodes. For each `v-for`, reports an argument,
//! modifiers, a missing value, or a destructured secondary/tertiary alias.
//! Then enforces the `:key` requirement: a custom component rendered with
//! `v-for` needs a `:key`, and any `:key` must reference an iteration variable.
//! For a `<template v-for>`, the requirement is checked on the child elements
//! that do not themselves iterate over a parent variable.

use crate::diagnostic::{Diagnostic, Severity};

const MSG_ARGUMENT: &str = "The v-for directive does not accept an argument.";
const MSG_MODIFIER: &str = "The v-for directive does not support modifiers.";
const MSG_MISSING_VALUE: &str = "The v-for directive requires a value.";
const MSG_SECONDARY_ALIAS: &str = "The second and third v-for aliases must be identifiers.";
const MSG_MISSING_KEY: &str =
    "Custom components rendered with v-for require a v-bind:key directive.";
const MSG_KEY_NO_VARS: &str =
    "This v-bind:key directive does not use any variables from the v-for directive.";

/// Find the first child of `node` whose kind is in `kinds`.
fn child_of_kind<'a>(node: tree_sitter::Node<'a>, kinds: &[&str]) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| kinds.contains(&c.kind()))
}

/// Read the `directive_name` text of a `directive_attribute`.
fn directive_name<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    child_of_kind(directive, &["directive_name"]).and_then(|n| n.utf8_text(source).ok())
}

/// Read the `attribute_value` (expression) text of a `directive_attribute`,
/// descending through a `quoted_attribute_value` wrapper. `None` when the
/// directive has no value or an empty quoted value.
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

/// The `directive_argument`/`directive_dynamic_argument` text of a binding
/// directive (`:foo`, `v-bind:foo`), e.g. `key` for `:key`.
fn binding_argument<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    child_of_kind(directive, &["directive_argument", "directive_dynamic_argument"])
        .and_then(|n| n.utf8_text(source).ok())
}

/// Whether a `directive_attribute` is a `:key` / `v-bind:key` binding.
fn is_key_binding(directive: tree_sitter::Node, source: &[u8]) -> bool {
    matches!(directive_name(directive, source), Some(":") | Some("v-bind"))
        && binding_argument(directive, source) == Some("key")
}

/// Split a `v-for` value into its `(alias, iterable)` halves on the top-level
/// `in`/`of` keyword (outside any bracket nesting).
fn split_for(value: &str) -> Option<(&str, &str)> {
    let bytes = value.as_bytes();
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'i' | b'o' if depth == 0 => {
                let after = i + 2;
                let kw = &value[i..after.min(value.len())];
                let prev_boundary = i == 0 || !is_id_char(bytes[i - 1]);
                let next_boundary = after >= bytes.len() || !is_id_char(bytes[after]);
                if (kw == "in" || kw == "of") && prev_boundary && next_boundary {
                    return Some((value[..i].trim(), value[after..].trim()));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn is_id_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

fn is_id_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Whether `expr` contains `ident` as a whole identifier token (not as a
/// substring of a longer identifier).
fn contains_identifier(expr: &str, ident: &str) -> bool {
    if ident.is_empty() {
        return false;
    }
    let bytes = expr.as_bytes();
    let needle = ident.as_bytes();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let before_ok = i == 0 || !is_id_char(bytes[i - 1]);
            let after = i + needle.len();
            let after_ok = after >= bytes.len() || !is_id_char(bytes[after]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Collect every identifier token appearing in an alias expression. This is a
/// superset of the variables a `v-for` binds (it also picks up object property
/// keys), which only makes `:key` checks more lenient and never over-fires.
fn collect_identifiers(alias: &str) -> Vec<&str> {
    let bytes = alias.as_bytes();
    let mut idents = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if is_id_start(bytes[i]) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_id_char(bytes[i]) {
                i += 1;
            }
            idents.push(&alias[start..i]);
        } else {
            i += 1;
        }
    }
    idents
}

/// The alias is a parenthesised tuple `(a, b, c)`. Return its top-level
/// comma-separated parts (trimmed), or `None` when it is not a tuple.
fn tuple_parts(alias: &str) -> Option<Vec<&str>> {
    let trimmed = alias.trim();
    if !(trimmed.starts_with('(') && trimmed.ends_with(')')) {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let bytes = inner.as_bytes();
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(inner[start..].trim());
    Some(parts)
}

/// Whether a tuple part is a destructuring pattern rather than a plain
/// identifier (the secondary/tertiary aliases must be identifiers).
fn is_destructuring(part: &str) -> bool {
    part.starts_with('{') || part.starts_with('[')
}

/// The 2nd and 3rd tuple aliases (if any) must be plain identifiers.
fn has_invalid_secondary_alias(alias: &str) -> bool {
    match tuple_parts(alias) {
        Some(parts) => parts.iter().skip(1).any(|p| is_destructuring(p)),
        None => false,
    }
}

/// The enclosing `start_tag` / `self_closing_tag` of a directive.
fn enclosing_tag(directive: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let tag = directive.parent()?;
    matches!(tag.kind(), "start_tag" | "self_closing_tag").then_some(tag)
}

/// The `tag_name` text of a `start_tag` / `self_closing_tag`.
fn tag_name<'a>(tag: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    child_of_kind(tag, &["tag_name"]).and_then(|n| n.utf8_text(source).ok())
}

/// Whether a tag is a custom component: a PascalCase or dashed name, a member
/// name (`Foo.Bar`), or an element carrying an `is` attribute / `:is` binding.
fn is_custom_component(tag: tree_sitter::Node, source: &[u8]) -> bool {
    let component_name = tag_name(tag, source).is_some_and(|name| {
        name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
            || name.contains('-')
            || name.contains('.')
    });
    component_name || has_is_attribute(tag, source)
}

/// Whether a tag carries a static `is` attribute or an `:is` / `v-bind:is`
/// binding.
fn has_is_attribute(tag: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = tag.walk();
    tag.children(&mut cursor).any(|child| match child.kind() {
        "attribute" => {
            child_of_kind(child, &["attribute_name"])
                .and_then(|n| n.utf8_text(source).ok())
                == Some("is")
        }
        "directive_attribute" => binding_argument(child, source) == Some("is"),
        _ => false,
    })
}

/// The `:key` binding directive on a tag, if present.
fn key_directive<'a>(tag: tree_sitter::Node<'a>, source: &[u8]) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = tag.walk();
    tag.children(&mut cursor)
        .find(|c| c.kind() == "directive_attribute" && is_key_binding(*c, source))
}

/// The `v-for` directive on a tag, if present.
fn v_for_directive<'a>(tag: tree_sitter::Node<'a>, source: &[u8]) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = tag.walk();
    tag.children(&mut cursor).find(|c| {
        c.kind() == "directive_attribute" && directive_name(*c, source) == Some("v-for")
    })
}

/// A `:key` violation for `tag`, evaluated against the iteration `bindings`:
/// `KeyDoesNotUseIterationVariables` when the present `:key` uses none of them,
/// or `MissingKey` when a custom component has no `:key`.
fn key_violation<'a>(
    tag: tree_sitter::Node<'a>,
    source: &[u8],
    bindings: &[&str],
) -> Option<(tree_sitter::Node<'a>, &'static str)> {
    if let Some(key) = key_directive(tag, source) {
        let uses_binding = directive_value(key, source).is_some_and(|expr| {
            bindings
                .iter()
                .any(|binding| contains_identifier(expr, binding))
        });
        return (!uses_binding).then_some((key, MSG_KEY_NO_VARS));
    }
    is_custom_component(tag, source).then_some((tag, MSG_MISSING_KEY))
}

/// The opening tag of a child element node (`element` / `template_element`).
fn child_tag(element: tree_sitter::Node) -> Option<tree_sitter::Node> {
    child_of_kind(element, &["start_tag", "self_closing_tag"])
}

/// For a `<template v-for>`, whether `child_tag` iterates over one of the
/// parent's `bindings` via its own `v-for` (in which case the parent need not
/// supply a key for it).
fn child_uses_parent_binding(child_tag: tree_sitter::Node, source: &[u8], bindings: &[&str]) -> bool {
    let Some(child_for) = v_for_directive(child_tag, source) else {
        return false;
    };
    let Some(value) = directive_value(child_for, source) else {
        return false;
    };
    let Some((_, iterable)) = split_for(value) else {
        return false;
    };
    bindings
        .iter()
        .any(|binding| contains_identifier(iterable, binding))
}

fn push(diagnostics: &mut Vec<Diagnostic>, node: tree_sitter::Node, ctx_path: &std::sync::Arc<std::path::Path>, message: &str) {
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(ctx_path),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: message.into(),
        severity: Severity::Error,
        span: None,
    });
}

crate::ast_check! { on ["directive_attribute"] prefilter = ["v-for"] => |node, source, ctx, diagnostics|
    if directive_name(node, source) != Some("v-for") {
        return;
    }

    // Directive-level violations: argument, modifiers, missing value, alias.
    if child_of_kind(node, &["directive_argument", "directive_dynamic_argument"]).is_some() {
        push(diagnostics, node, &ctx.path_arc, MSG_ARGUMENT);
        return;
    }
    if child_of_kind(node, &["directive_modifiers"]).is_some() {
        push(diagnostics, node, &ctx.path_arc, MSG_MODIFIER);
        return;
    }
    let Some(value) = directive_value(node, source) else {
        push(diagnostics, node, &ctx.path_arc, MSG_MISSING_VALUE);
        return;
    };
    let Some((alias, _iterable)) = split_for(value) else {
        return;
    };
    if has_invalid_secondary_alias(alias) {
        push(diagnostics, node, &ctx.path_arc, MSG_SECONDARY_ALIAS);
        return;
    }

    let bindings = collect_identifiers(alias);
    let Some(tag) = enclosing_tag(node) else {
        return;
    };

    // `<template v-for>` checks the child elements; everything else checks the
    // element carrying the directive.
    if tag_name(tag, source) == Some("template") {
        // A `:key` on the `<template>` itself satisfies the requirement; the
        // children are not checked individually.
        if key_directive(tag, source).is_some() {
            return;
        }
        let Some(container) = tag.parent() else {
            return;
        };
        let mut cursor = container.walk();
        for child in container.children(&mut cursor) {
            if !matches!(child.kind(), "element" | "template_element") {
                continue;
            }
            let Some(ctag) = child_tag(child) else {
                continue;
            };
            if child_uses_parent_binding(ctag, source, &bindings) {
                continue;
            }
            if let Some((target, message)) = key_violation(ctag, source, &bindings) {
                push(diagnostics, target, &ctx.path_arc, message);
            }
        }
    } else if let Some((target, message)) = key_violation(tag, source, &bindings) {
        push(diagnostics, target, &ctx.path_arc, message);
    }
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
        let diags = run(&wrap("<div v-for:arg=\"item in items\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not accept an argument"));
    }

    #[test]
    fn flags_modifier() {
        let diags = run(&wrap("<div v-for.mod=\"item in items\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not support modifiers"));
    }

    #[test]
    fn flags_bare_missing_value() {
        let diags = run(&wrap("<div v-for></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("requires a value"));
    }

    #[test]
    fn flags_empty_value() {
        let diags = run(&wrap("<div v-for=\"\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("requires a value"));
    }

    #[test]
    fn flags_destructured_secondary_alias() {
        let diags = run(&wrap("<div v-for=\"(item, { key }) in items\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("second and third"));
    }

    #[test]
    fn flags_custom_component_without_key() {
        let diags = run(&wrap("<MyItem v-for=\"item in items\"></MyItem>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("require a v-bind:key"));
    }

    #[test]
    fn flags_is_attribute_component_without_key() {
        let diags = run(&wrap("<div is=\"MyItem\" v-for=\"item in items\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("require a v-bind:key"));
    }

    #[test]
    fn flags_key_not_using_iteration_variables() {
        let diags = run(&wrap("<div v-for=\"item in items\" :key=\"foo\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not use any variables"));
    }

    #[test]
    fn flags_template_child_custom_component_without_key() {
        let source = wrap("<template v-for=\"item in items\">\n<MyItem />\n</template>");
        let diags = run(&source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("require a v-bind:key"));
    }

    #[test]
    fn flags_template_child_key_not_using_parent_variable() {
        let source = wrap(
            "<template v-for=\"item in items\">\n\
             <div v-for=\"child in other\" :key=\"child.id\"></div>\n\
             </template>",
        );
        let diags = run(&source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not use any variables"));
    }

    #[test]
    fn flags_all_biome_invalid_lint_cases() {
        // Mirrors Biome's invalid.vue, excluding the two parse-error-only lines
        // (`(, index)` produces no lint diagnostic). 10 lint diagnostics total.
        let source = wrap(
            "<div v-for:arg=\"item in items\"></div>\n\
             <div v-for.mod=\"item in items\"></div>\n\
             <div v-for></div>\n\
             <div v-for=\"\"></div>\n\
             <div v-for=\"(item, { key }) in items\"></div>\n\
             <MyItem v-for=\"item in items\"></MyItem>\n\
             <div is=\"MyItem\" v-for=\"item in items\"></div>\n\
             <div v-for=\"item in items\" :key=\"foo\"></div>\n\
             <template v-for=\"item in items\">\n<MyItem />\n</template>\n\
             <template v-for=\"item in items\">\n\
             <div v-for=\"child in other\" :key=\"child.id\"></div>\n\
             </template>",
        );
        assert_eq!(run(&source).len(), 10);
    }

    // --- Valid fixtures (Biome valid.vue) ---

    #[test]
    fn allows_plain_element_without_key() {
        assert!(run(&wrap("<div v-for=\"item in items\"></div>")).is_empty());
    }

    #[test]
    fn allows_destructured_primary_alias_with_matching_key() {
        let diags = run(&wrap(
            "<div v-for=\"({ id }, index) in items\" :key=\"id\"></div>",
        ));
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_custom_component_with_matching_key() {
        assert!(run(&wrap("<MyItem v-for=\"item in items\" :key=\"item.id\" />")).is_empty());
    }

    #[test]
    fn allows_dynamic_is_binding_with_matching_key() {
        let diags = run(&wrap(
            "<div :is=\"componentName\" v-for=\"item in items\" :key=\"item.id\"></div>",
        ));
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_template_with_key_on_template() {
        let source = wrap(
            "<template v-for=\"item in items\" :key=\"item.id\">\n<MyItem />\n</template>",
        );
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_template_child_with_matching_key() {
        let source = wrap(
            "<template v-for=\"item in items\">\n<div :key=\"item.id\"></div>\n</template>",
        );
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_template_child_iterating_parent_variable() {
        let source = wrap(
            "<template v-for=\"item in items\">\n\
             <div v-for=\"child in item.children\" :key=\"child.id\"></div>\n\
             </template>",
        );
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_template_child_plain_div_without_key() {
        let source =
            wrap("<template v-for=\"item in items\">\n<div></div>\n</template>");
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_all_biome_valid_fixtures() {
        let source = wrap(
            "<div v-for=\"item in items\"></div>\n\
             <div v-for=\"({ id }, index) in items\" :key=\"id\"></div>\n\
             <MyItem v-for=\"item in items\" :key=\"item.id\" />\n\
             <div :is=\"componentName\" v-for=\"item in items\" :key=\"item.id\"></div>\n\
             <template v-for=\"item in items\" :key=\"item.id\">\n<MyItem />\n</template>\n\
             <template v-for=\"item in items\">\n<div :key=\"item.id\"></div>\n</template>\n\
             <template v-for=\"item in items\">\n<div v-for=\"child in item.children\" :key=\"child.id\"></div>\n</template>\n\
             <template v-for=\"item in items\">\n<div class=\"nested\" v-for=\"child in item.children\" :key=\"child.id\"></div>\n</template>\n\
             <template v-for=\"item in items\">\n<div></div>\n</template>",
        );
        assert!(run(&source).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_v_for_with_of_keyword() {
        assert!(run(&wrap("<div v-for=\"item of items\"></div>")).is_empty());
    }

    #[test]
    fn ignores_other_directives() {
        assert!(run(&wrap("<div v-if=\"ok\" :id=\"x\"></div>")).is_empty());
    }

    #[test]
    fn key_token_not_matched_as_substring() {
        // `items` contains `item` as a substring but not as a whole token, so a
        // key referencing `itemList` must still flag.
        let diags = run(&wrap("<div v-for=\"item in items\" :key=\"itemList\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not use any variables"));
    }
}
