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
