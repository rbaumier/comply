//! Shared helpers for Rust tree-sitter rules.
//!
//! Extracted because three independent rules need the same
//! "are we inside an async function" check (`thread-sleep-in-async`,
//! `block-on-in-async`, `sync-io-in-async`). Rule of three: extract.

use tree_sitter::Node;

/// True if `node` is inside an `async fn`. Walks up parents looking
/// for the nearest `function_item` and checks whether its signature
/// text contains the `async` keyword. We use a text scan rather than
/// a field lookup because tree-sitter-rust doesn't expose `async` as
/// a named field — it's an anonymous keyword child of `function_item`.
///
/// Closures (`async move { … }`) are not handled here on purpose:
/// the typical footgun is calling sync APIs from `async fn` bodies,
/// not from short-lived async blocks.
pub fn is_inside_async_fn(node: Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            // Read the signature up to the body's `{` so we don't scan
            // the entire function body for the keyword `async`.
            let body_start = parent
                .child_by_field_name("body")
                .map(|b| b.start_byte())
                .unwrap_or(parent.end_byte());
            let sig_start = parent.start_byte();
            let signature = &source[sig_start..body_start];
            if let Ok(text) = std::str::from_utf8(signature)
                && text.contains("async")
            {
                return true;
            }
            // We found the enclosing fn but it's not async — stop
            // walking, nested fns can't change the answer.
            return false;
        }
        cur = parent;
    }
    false
}

/// If `node` is a `Result<T, E>` `generic_type`, return its second
/// positional type argument (the error type `E`). Returns `None` for
/// any other node, or for `Result<T>` aliases like `io::Result<T>`
/// where the error type isn't visible from the AST.
///
/// Both `rust-string-as-error` and `rust-unit-error-result` need this
/// "find the error type" walk — without it they reimplemented the
/// same generic-arg traversal in two places.
pub fn result_error_type<'a>(node: Node<'a>, source: &[u8]) -> Option<Node<'a>> {
    if node.kind() != "generic_type" {
        return None;
    }
    let type_node = node.child_by_field_name("type")?;
    let type_text = type_node.utf8_text(source).ok()?;
    if type_text != "Result" && !type_text.ends_with("::Result") {
        return None;
    }
    let args = node.child_by_field_name("type_arguments")?;
    let mut cursor = args.walk();
    let positional: Vec<_> = args
        .named_children(&mut cursor)
        .filter(|c| c.kind() != "type_binding")
        .collect();
    if positional.len() < 2 {
        return None;
    }
    Some(positional[1])
}

/// True if `node` is inside any form of Rust test context:
///
/// - inside a `#[test]` function
/// - inside a `#[cfg(test)]` / `#[cfg_attr(test, …)]` module
/// - inside a file marked with `#![cfg(test)]`
///
/// Rules that want to relax their discipline for test code (allow
/// `unwrap`, `panic!`, `let _ = fallible()`, etc.) call this helper
/// to decide whether a candidate should be skipped.
pub fn is_in_test_context(node: Node, source: &[u8]) -> bool {
    // File-level inner attribute: `#![cfg(test)]` on the crate root.
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() != "inner_attribute_item" {
            continue;
        }
        if let Ok(text) = child.utf8_text(source)
            && text.contains("cfg(test)")
        {
            return true;
        }
    }

    // Outer `#[test]` / `#[cfg(test)]` on an enclosing function or module.
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if (parent.kind() == "function_item" || parent.kind() == "mod_item")
            && has_test_attribute(parent, source)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if the item has `#[test]`, `#[cfg(test)]`, or `#[cfg_attr(test, …)]`
/// as a preceding `attribute_item` sibling. In tree-sitter-rust, outer
/// attributes on an item appear as `attribute_item` nodes immediately
/// before the item they decorate.
pub fn has_test_attribute(item: Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && (text.contains("#[test]")
                || text.contains("cfg(test)")
                || text.contains("cfg_attr(test"))
        {
            return true;
        }
        sibling = s.prev_named_sibling();
    }
    false
}

