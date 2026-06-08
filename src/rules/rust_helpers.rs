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

/// True if `node` is inside a closure that is passed directly as an argument
/// to a thread-spawning function (`thread::spawn`, `spawn_blocking`, etc.).
/// Those closures execute on a separate OS thread, not on the async runtime
/// worker, so blocking calls inside them are safe.
pub fn is_inside_spawned_closure(node: Node, source: &[u8]) -> bool {
    use crate::rules::call_expression::call_function_name;
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            return false;
        }
        if parent.kind() == "closure_expression" {
            if let Some(args) = parent.parent()
                && args.kind() == "arguments"
                && let Some(call) = args.parent()
                && call.kind() == "call_expression"
                && let Some(fn_text) = call_function_name(call, source)
                && is_thread_spawn_fn(fn_text)
            {
                return true;
            }
        }
        cur = parent;
    }
    false
}

fn is_thread_spawn_fn(text: &str) -> bool {
    text.ends_with("thread::spawn")
        || text.contains("thread::Builder")
        || text.ends_with("spawn_blocking")
        || text.ends_with("rayon::spawn")
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

/// True if `path` is under a `tests/` directory component — i.e. any
/// path segment equals `"tests"`. Covers `<crate>/tests/`, `tests/` at
/// the workspace root, and deeply nested `foo/tests/` subdirectories.
///
/// Shared by rules that want to skip diagnostics for integration-test
/// files without relying on the tree-sitter attribute walk.
pub fn is_under_tests_dir(path: &std::path::Path) -> bool {
    path.components().any(|c| c.as_os_str() == "tests")
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
                || text.contains("cfg_attr(test")
                || text.contains("::test]")   // #[tokio::test], #[actix_rt::test], …
                || text.contains("::test("))  // #[tokio::test(flavor = "multi_thread")], …
        {
            return true;
        }
        sibling = s.prev_named_sibling();
    }
    false
}
