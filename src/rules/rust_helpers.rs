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

// `is_inside_async_fn` is exercised end-to-end via the three rules
// that depend on it (`rust-thread-sleep-in-async`, `rust-block-on-in-async`,
// `rust-sync-io-in-async`). Their backend test suites cover both the
// async-fn-positive and sync-fn-negative cases, so a unit test here
// would duplicate that coverage.
