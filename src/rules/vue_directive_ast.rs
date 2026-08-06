//! Shared AST helpers for Vue template directives.
//!
//! The `tree-sitter-vue-updated` grammar exposes every `v-` attribute and every
//! `:`/`@`/`#` shorthand as a `directive_attribute` node holding a
//! `directive_name`, an optional `directive_argument` (or
//! `directive_dynamic_argument`), optional `directive_modifiers`, and an
//! optional value (bare `attribute_value` or a `quoted_attribute_value`
//! wrapper). These helpers read that shape so rules about the same construct
//! share one parse instead of each re-deriving it from raw text.

/// Find the first child of `node` whose kind is in `kinds`.
pub fn child_of_kind<'a>(
    node: tree_sitter::Node<'a>,
    kinds: &[&str],
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| kinds.contains(&c.kind()))
}

/// Read the `directive_name` text of a `directive_attribute` (`v-for`, `v-if`,
/// `:` for the `v-bind` shorthand, `@` for the `v-on` shorthand).
pub fn directive_name<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    child_of_kind(directive, &["directive_name"]).and_then(|n| n.utf8_text(source).ok())
}

/// Read the `attribute_value` (expression) text of a `directive_attribute`,
/// descending through a `quoted_attribute_value` wrapper. `None` when the
/// directive has no value or an empty quoted value.
pub fn directive_value<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
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
/// directive (`:foo`, `v-bind:foo`), e.g. `key` for `:key`. A dynamic argument
/// comes back bracketed (`[foo]` for `:[foo]`), so it never equals a bare name.
pub fn binding_argument<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    child_of_kind(directive, &["directive_argument", "directive_dynamic_argument"])
        .and_then(|n| n.utf8_text(source).ok())
}

/// Whether a binding directive uses the Vue 3.5+ same-name shorthand: it carries
/// an argument but no value node at all (`:key`, as opposed to `:key="x"` or an
/// empty `:key=""`, both of which have a value node).
fn is_same_name_shorthand(directive: tree_sitter::Node) -> bool {
    let mut cursor = directive.walk();
    !directive
        .children(&mut cursor)
        .any(|c| matches!(c.kind(), "attribute_value" | "quoted_attribute_value"))
}

/// The expression a binding directive binds. Under the Vue 3.5+ same-name
/// shorthand a valueless `:key` binds its own argument name (`:key` is
/// `:key="key"`), while an empty `:key=""` carries a value node and binds
/// nothing. Both rules reading a `:key` resolve it here, so they cannot disagree
/// about which expression the attribute names.
pub fn bound_expression<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    directive_value(directive, source).or_else(|| {
        is_same_name_shorthand(directive)
            .then(|| binding_argument(directive, source))
            .flatten()
    })
}

/// Whether a `directive_attribute` is a `:key` / `v-bind:key` binding.
fn is_key_binding(directive: tree_sitter::Node, source: &[u8]) -> bool {
    matches!(directive_name(directive, source), Some(":") | Some("v-bind"))
        && binding_argument(directive, source) == Some("key")
}

/// The `:key` binding directive on a tag, if present.
pub fn key_directive<'a>(
    tag: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = tag.walk();
    tag.children(&mut cursor)
        .find(|c| c.kind() == "directive_attribute" && is_key_binding(*c, source))
}

/// The enclosing `start_tag` / `self_closing_tag` of a directive.
pub fn enclosing_tag(directive: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let tag = directive.parent()?;
    matches!(tag.kind(), "start_tag" | "self_closing_tag").then_some(tag)
}

/// Whether a byte can continue a JavaScript identifier.
pub fn is_id_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Split a `v-for` value into its `(alias, iterable)` halves on the top-level
/// `in`/`of` keyword (outside any bracket nesting), so an `in`/`of` inside the
/// alias tuple or a nested expression does not split.
///
/// The keyword compare is guarded by `is_char_boundary`: a multi-byte character
/// right after an ASCII `i`/`o` (`v-for="oété in x"`) would otherwise slice
/// mid-codepoint and panic.
pub fn split_for(value: &str) -> Option<(&str, &str)> {
    let bytes = value.as_bytes();
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'i' | b'o' if depth == 0 => {
                let after = (i + 2).min(value.len());
                if value.is_char_boundary(after) {
                    let kw = &value[i..after];
                    let prev_boundary = i == 0 || !is_id_char(bytes[i - 1]);
                    let next_boundary = after >= bytes.len() || !is_id_char(bytes[after]);
                    if (kw == "in" || kw == "of") && prev_boundary && next_boundary {
                        return Some((value[..i].trim(), value[after..].trim()));
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The alias half of a `v-for` as a parenthesised tuple `(a, b, c)`. Returns its
/// top-level comma-separated parts (trimmed), or `None` when the alias is a bare
/// binding (`v-for="item in items"`), which declares no secondary alias.
pub fn tuple_parts(alias: &str) -> Option<Vec<&str>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_for_splits_on_top_level_in() {
        assert_eq!(split_for("(item, i) in items"), Some(("(item, i)", "items")));
    }

    #[test]
    fn split_for_splits_on_top_level_of() {
        assert_eq!(split_for("item of items"), Some(("item", "items")));
    }

    #[test]
    fn split_for_ignores_in_inside_brackets() {
        // The `in` inside the parenthesised alias must not split the value.
        assert_eq!(
            split_for("(inner, i) in list.filter(x => 'a' in x)"),
            Some(("(inner, i)", "list.filter(x => 'a' in x)"))
        );
    }

    #[test]
    fn split_for_requires_whole_keyword() {
        assert_eq!(split_for("info"), None);
    }

    #[test]
    fn split_for_survives_multibyte_after_ascii_i_or_o() {
        // Regression for #7663: after an ASCII `i`/`o`, `i + 2` can land
        // mid-codepoint; the boundary guard keeps the slice valid.
        assert_eq!(split_for("oété in x"), Some(("oété", "x")));
        assert_eq!(split_for("ié in x"), Some(("ié", "x")));
        // No keyword at all, ending on a multi-byte char.
        assert_eq!(split_for("ré"), None);
    }

    #[test]
    fn tuple_parts_splits_top_level_commas() {
        assert_eq!(
            tuple_parts("(value, key, index)"),
            Some(vec!["value", "key", "index"])
        );
    }

    #[test]
    fn tuple_parts_keeps_nested_destructuring_together() {
        assert_eq!(tuple_parts("({ id, name }, i)"), Some(vec!["{ id, name }", "i"]));
    }

    #[test]
    fn tuple_parts_none_for_bare_alias() {
        assert_eq!(tuple_parts("item"), None);
    }
}
