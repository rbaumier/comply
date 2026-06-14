//! Shared helpers for Rust tree-sitter rules.
//!
//! Extracted because three independent rules need the same
//! "are we inside an async function" check (`thread-sleep-in-async`,
//! `block-on-in-async`, `sync-io-in-async`). Rule of three: extract.

use tree_sitter::Node;

/// True if `node` is inside an `async fn`. Walks up parents looking
/// for the nearest `function_item` and inspects its `function_modifiers`
/// child for the `async` keyword. tree-sitter-rust groups `async`,
/// `const`, `unsafe`, `extern "C"` etc. under a `function_modifiers`
/// node, so a sync function never has `async` there — even one named
/// with a raw identifier (`fn r#async()`), whose `async` lives only in
/// the `name` field, not in any modifier.
///
/// Closures (`async move { … }`) are not handled here on purpose:
/// the typical footgun is calling sync APIs from `async fn` bodies,
/// not from short-lived async blocks.
pub fn is_inside_async_fn(node: Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            return fn_modifiers_contain_async(parent, source);
        }
        cur = parent;
    }
    false
}

/// True if a `function_item`'s `function_modifiers` child contains the
/// `async` keyword. Scans the modifiers node only, so raw identifiers
/// (`fn r#async()`), parameter types, and return types named "async"
/// can't trip the check.
fn fn_modifiers_contain_async(function_item: Node, source: &[u8]) -> bool {
    let mut cursor = function_item.walk();
    for child in function_item.children(&mut cursor) {
        if child.kind() == "function_modifiers" {
            return child
                .utf8_text(source)
                .is_ok_and(|text| text.split_whitespace().any(|word| word == "async"));
        }
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

/// True if `node` sits inside a trait implementation (`impl Trait for Type`).
///
/// Walks up via `node.parent()` to the *nearest* enclosing `impl_item` and
/// returns whether that impl has a `trait` field. The decision is made for the
/// nearest impl only: an inherent `impl Type { … }` returns `false`, and a node
/// with no enclosing impl returns `false`. Rules use this to exempt methods
/// whose shape is forced by a trait contract (the implementor can't change it).
pub fn is_in_trait_impl(node: Node) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            return ancestor.child_by_field_name("trait").is_some();
        }
        current = ancestor.parent();
    }
    false
}

/// True if `item` is publicly visible outside the crate, i.e. it carries a bare
/// `pub` visibility modifier.
///
/// Canonical semantics: ONLY bare `pub` counts as public. Restricted forms —
/// `pub(crate)`, `pub(super)`, and `pub(in path)` — are treated as NON-public,
/// because the consuming rules only care about items reachable from outside the
/// crate. The `.trim() == "pub"` comparison is whitespace-robust; the restricted
/// forms carry their `(…)` qualifier in the modifier text and never trim to
/// `"pub"`.
pub fn is_pub(item: Node, source: &[u8]) -> bool {
    let mut cursor = item.walk();
    for child in item.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && let Ok(text) = child.utf8_text(source)
        {
            return text.trim() == "pub";
        }
    }
    false
}

/// True if the `match_arm`'s body is a single diverging or error
/// expression — a `unreachable!`/`panic!`/`unimplemented!`/`todo!`/`bail!`
/// macro invocation, or a `return Err(...)`. Such an arm is an explicit
/// guard for the impossible/error case.
///
/// Two rules need this: `rust-explicit-enum-match-arms` exempts a
/// wildcard arm that only diverges, and `no-empty-catch` treats an empty
/// `Err(_) => {}` arm as a controlled assertion (not error-swallowing)
/// when a sibling arm diverges.
pub fn arm_body_is_diverging(arm: Node, source: &[u8]) -> bool {
    let Some(value) = arm.child_by_field_name("value") else {
        return false;
    };
    expr_is_diverging(value, source)
}

/// Classify a match-arm body expression as diverging/error. A `block`
/// body with a single statement is unwrapped to its inner expression so
/// `{ bail!("…"); }` is treated like `bail!("…")`.
fn expr_is_diverging(expr: Node, source: &[u8]) -> bool {
    match expr.kind() {
        "block" => {
            // Only an unconditional single-statement body is a guard:
            // `{ bail!("…"); }` or `{ return Err(e); }`. A block doing
            // other work before diverging is a real catch-all.
            let mut cursor = expr.walk();
            let mut children = expr.named_children(&mut cursor);
            let (Some(only), None) = (children.next(), children.next()) else {
                return false;
            };
            let inner = if only.kind() == "expression_statement" {
                match only.named_child(0) {
                    Some(node) => node,
                    None => return false,
                }
            } else {
                only
            };
            expr_is_diverging(inner, source)
        }
        "macro_invocation" => {
            let Some(name_node) = expr.child_by_field_name("macro") else {
                return false;
            };
            matches!(
                name_node.utf8_text(source),
                Ok("unreachable" | "panic" | "unimplemented" | "todo" | "bail")
            )
        }
        "return_expression" => return_yields_err(expr, source),
        _ => false,
    }
}

/// True if a `return_expression` returns an `Err(...)` value — the head
/// of the returned call expression is the `Err` constructor.
fn return_yields_err(ret: Node, source: &[u8]) -> bool {
    let Some(returned) = ret.named_child(0) else {
        return false;
    };
    if returned.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = returned.child_by_field_name("function") else {
        return false;
    };
    let Ok(text) = callee.utf8_text(source) else {
        return false;
    };
    text.rsplit("::").next().unwrap_or(text).trim() == "Err"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("grammar should load");
        parser
            .parse(source, None)
            .expect("parser should produce a tree")
    }

    /// Find the first `function_item` node anywhere in the tree.
    fn first_function_item(node: Node) -> Option<Node> {
        if node.kind() == "function_item" {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = first_function_item(child) {
                return Some(found);
            }
        }
        None
    }

    /// Find the first `call_expression` node anywhere in the tree.
    fn first_call_expression(node: Node) -> Option<Node> {
        if node.kind() == "call_expression" {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = first_call_expression(child) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn canonical_is_pub_excludes_pub_crate_and_pub_super() {
        let cases = [
            ("pub fn f() {}", true),
            ("pub(crate) fn f() {}", false),
            ("pub(super) fn f() {}", false),
            ("pub(in crate::a) fn f() {}", false),
            ("fn f() {}", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let func = first_function_item(tree.root_node())
                .expect("snippet should contain a function_item");
            assert_eq!(
                is_pub(func, src.as_bytes()),
                expected,
                "is_pub mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_in_trait_impl_distinguishes_trait_from_inherent() {
        let trait_impl = "struct T; impl Tr for T { fn m(&self) {} }";
        let inherent_impl = "struct T; impl T { fn m(&self) {} }";
        let free_fn = "fn m() {}";

        let cases = [
            (trait_impl, true),
            (inherent_impl, false),
            (free_fn, false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let func = first_function_item(tree.root_node())
                .expect("snippet should contain a function_item");
            assert_eq!(
                is_in_trait_impl(func),
                expected,
                "is_in_trait_impl mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_inside_async_fn_distinguishes_async_from_raw_identifier() {
        let cases = [
            ("async fn f() { g(); }", true),
            ("pub async fn f() { g(); }", true),
            ("fn r#async() { g(); }", false),
            ("fn f() { g(); }", false),
            ("fn f(r#async: u8) { g(); }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let call = first_call_expression(tree.root_node())
                .expect("snippet should contain a call_expression");
            assert_eq!(
                is_inside_async_fn(call, src.as_bytes()),
                expected,
                "is_inside_async_fn mismatch for `{src}`"
            );
        }
    }
}
