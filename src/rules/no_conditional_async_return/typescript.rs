//! no-conditional-async-return backend — flag functions that mix sync and
//! promise-returning branches.
//!
//! Why: a function whose return type is `T | Promise<T>` forces every
//! caller to treat it as async, erasing the benefit of the fast path. And
//! callers who forget the `await` get silently wrong behaviour on the
//! promise branch. Pick one: async throughout, or sync throughout.
//!
//! Detection: for each function-like node, collect its `return_statement`
//! descendants *without* crossing another function boundary. Each return
//! is classified as:
//!   - promise: the returned expression is `foo.then(...)`, `foo.catch(...)`,
//!     or `Promise.resolve/reject/all/allSettled/race/any(...)`.
//!   - sync: any other value.
//!   - void: bare `return;` — ignored.
//!
//! Flag the function when it has at least one sync return AND at least
//! one promise return. `async` functions are skipped because their returns
//! are promise-wrapped by the runtime.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReturnKind {
    Sync,
    Promise,
}

/// Is this node an `async` function? Tree-sitter exposes `async` as an
/// anonymous child token, not a field, so scan children for the keyword.
fn is_async_function(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.utf8_text(source).unwrap_or("") == "async" {
            return true;
        }
    }
    false
}

/// Classify a return-value expression as promise-returning or sync.
fn classify_value(value: tree_sitter::Node, source: &[u8]) -> ReturnKind {
    if value.kind() != "call_expression" {
        return ReturnKind::Sync;
    }
    let Some(func) = value.child_by_field_name("function") else {
        return ReturnKind::Sync;
    };
    if func.kind() != "member_expression" {
        return ReturnKind::Sync;
    }
    let Some(obj) = func.child_by_field_name("object") else {
        return ReturnKind::Sync;
    };
    let Some(prop) = func.child_by_field_name("property") else {
        return ReturnKind::Sync;
    };
    let method = prop.utf8_text(source).unwrap_or("");

    // `.then(...)` / `.catch(...)` on any receiver → promise.
    if method == "then" || method == "catch" || method == "finally" {
        return ReturnKind::Promise;
    }

    // `Promise.<combinator>(...)` → promise.
    if obj.utf8_text(source).unwrap_or("") == "Promise"
        && matches!(
            method,
            "resolve" | "reject" | "all" | "allSettled" | "race" | "any"
        )
    {
        return ReturnKind::Promise;
    }

    ReturnKind::Sync
}

/// Walk descendants of `body`, collecting return kinds. Skip subtrees
/// rooted at a nested function — those returns belong to a different
/// function.
fn collect_return_kinds(body: tree_sitter::Node, source: &[u8]) -> Vec<ReturnKind> {
    let mut out = Vec::new();
    let mut cursor = body.walk();
    if !cursor.goto_first_child() {
        return out;
    }
    loop {
        let node = cursor.node();

        if node.is_error() || node.is_missing() {
            if !advance(&mut cursor, body) {
                return out;
            }
            continue;
        }

        let is_fn_boundary = FUNCTION_KINDS.contains(&node.kind());

        if !is_fn_boundary && node.kind() == "return_statement" {
            // return_statement: first named child is the value, if any.
            if let Some(value) = node.named_child(0) {
                out.push(classify_value(value, source));
            }
        }

        // Don't descend into nested functions.
        if !is_fn_boundary && cursor.goto_first_child() {
            continue;
        }

        if !advance(&mut cursor, body) {
            return out;
        }
    }
}

/// Advance cursor to next sibling, walking up, but never past `body`.
fn advance(cursor: &mut tree_sitter::TreeCursor, body: tree_sitter::Node) -> bool {
    loop {
        if cursor.goto_next_sibling() {
            return true;
        }
        if !cursor.goto_parent() {
            return false;
        }
        if cursor.node().id() == body.id() {
            return false;
        }
    }
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(FUNCTION_KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        if is_async_function(node, source) {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        // Arrow with expression body (no statement_block) cannot mix
        // branches at the return-statement level — skip it.
        if body.kind() != "statement_block" {
            return;
        }
        let kinds = collect_return_kinds(body, source);
        let has_sync = kinds.contains(&ReturnKind::Sync);
        let has_promise = kinds.contains(&ReturnKind::Promise);
        if !(has_sync && has_promise) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "Function mixes sync and promise-returning branches — unify to `Promise<T>` (async) or plain `T` everywhere.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_mixed_promise_resolve_and_sync() {
        let src = "function f(x: boolean) { if (x) return 1; else return Promise.resolve(2); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_mixed_cached_and_then() {
        let src = "function f(x: boolean) { if (x) return cached; return fetch('/').then(r => r.json()); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_all_sync() {
        assert!(run("function f(x: number) { return x ? 1 : 2; }").is_empty());
    }

    #[test]
    fn allows_async_function() {
        assert!(run("async function f() { return 1; }").is_empty());
    }

    #[test]
    fn allows_all_promise() {
        assert!(run("function f() { return fetch('/').then(r => r.json()); }").is_empty());
    }
}
