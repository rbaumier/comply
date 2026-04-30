//! Shared helpers for JSX/TSX tree-sitter rules.
//!
//! Multiple rules walk `jsx_attribute` nodes to extract the attribute
//! name (`dangerouslySetInnerHTML`, `key`, …) and dispatch on it. Each
//! rule previously carried its own copy of the same 8-line walker. This
//! module is the single source of truth.

/// Extract the attribute name from a `jsx_attribute` node, e.g. `key`
/// from `<Foo key={i} />`. Returns `None` for any node that isn't a
/// JSX attribute or whose first child can't be read as UTF-8.
#[must_use]
pub fn jsx_attribute_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "jsx_attribute" {
        return None;
    }
    let name_node = node.child(0)?;
    name_node.utf8_text(source).ok()
}

/// Extract the value node from a `jsx_attribute` node.
///
/// tree-sitter TSX does NOT expose a `"value"` field name on
/// `jsx_attribute` — `child_by_field_name("value")` returns `None`.
/// The layout is: `property_identifier = <value>`, so the value
/// is the child after the `=` sign (typically child(2)).
#[must_use]
pub fn jsx_attribute_value(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if node.kind() != "jsx_attribute" {
        return None;
    }
    // Walk children looking for the node after '='
    let mut cursor = node.walk();
    let mut found_eq = false;
    for child in node.children(&mut cursor) {
        if found_eq {
            return Some(child);
        }
        if child.kind() == "=" {
            found_eq = true;
        }
    }
    None
}

/// Extract the string value (unquoted) from a `jsx_attribute` node.
///
/// Returns `None` if the value is not a string literal or can't be read.
#[must_use]
pub fn jsx_attribute_string_value<'a>(
    attr: tree_sitter::Node,
    source: &'a [u8],
) -> Option<&'a str> {
    let val = jsx_attribute_value(attr)?;
    if val.kind() != "string" {
        return None;
    }
    let text = val.utf8_text(source).ok()?;
    // Strip surrounding quotes
    Some(text.trim_matches(|c| c == '"' || c == '\''))
}

/// Get the tag name from a `jsx_self_closing_element` or `jsx_opening_element`.
///
/// In tree-sitter TSX, `child(0)` is `<`, not the tag name.
/// The tag name is available via `child_by_field_name("name")`.
#[must_use]
pub fn jsx_element_tag_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let kind = node.kind();
    if kind != "jsx_self_closing_element" && kind != "jsx_opening_element" {
        return None;
    }
    let name_node = node.child_by_field_name("name")?;
    name_node.utf8_text(source).ok()
}
