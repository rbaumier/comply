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

// Both helpers above are exercised end-to-end via the rules that
// depend on them (`rust-thread-sleep-in-async`, `rust-block-on-in-async`,
// `rust-sync-io-in-async`, `rust-string-as-error`, `rust-unit-error-result`).
// Their backend test suites cover both the positive and negative cases,
// so unit tests here would duplicate that coverage.
