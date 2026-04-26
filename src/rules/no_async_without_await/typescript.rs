//! no-async-without-await backend — flag `async` functions that contain no
//! `await` or `for await` in their own body.
//!
//! We walk every function-like node, check if it has the `async` keyword,
//! then scan descendants for any `await_expression` or `for_in_statement`
//! (which covers `for await (... of ...)` in tree-sitter-typescript). The
//! scan stops at nested function boundaries so an inner `async` function's
//! missing await is reported against that inner function, not the outer.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

fn is_async_function(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.utf8_text(source).unwrap_or("") == "async" {
            return true;
        }
    }
    false
}

/// Scan the function body for `await_expression` / `yield` of a promise,
/// stopping at nested function boundaries. Returns true if an `await`
/// (including `for await`) is found.
fn body_has_await(body: tree_sitter::Node, source: &[u8]) -> bool {
    let mut found = false;
    let mut cursor = body.walk();
    let mut stack: Vec<tree_sitter::Node> = Vec::new();
    for child in body.children(&mut cursor) {
        stack.push(child);
    }
    while let Some(n) = stack.pop() {
        // Don't descend into nested functions — their awaits don't count.
        if n.id() != body.id() && FUNCTION_KINDS.contains(&n.kind()) {
            continue;
        }
        if n.kind() == "await_expression" {
            found = true;
            break;
        }
        // `for await (x of y)` surfaces in tree-sitter as a for_in_statement
        // with an `await` anonymous child.
        if n.kind() == "for_in_statement" {
            let mut c2 = n.walk();
            for sub in n.children(&mut c2) {
                if sub.utf8_text(source).unwrap_or("") == "await" {
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }
        }
        let mut c = n.walk();
        for child in n.children(&mut c) {
            stack.push(child);
        }
    }
    found
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
        if !is_async_function(node, source) {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        if body_has_await(body, source) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-async-without-await".into(),
            message: "`async` function never awaits — drop the `async` keyword \
                      or add the `await` that justifies it."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_async_fn_without_await() {
        let d = run_on("async function f() { return 42; }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-async-without-await");
    }

    #[test]
    fn flags_async_arrow_without_await() {
        let d = run_on("const f = async () => { return 42; };");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_async_method_without_await() {
        let d = run_on("class C { async m() { return 1; } }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_async_fn_with_await() {
        assert!(run_on("async function f() { await g(); }").is_empty());
    }

    #[test]
    fn allows_async_fn_with_for_await() {
        assert!(run_on("async function f() { for await (const x of it) {} }").is_empty());
    }

    #[test]
    fn allows_non_async_fn() {
        assert!(run_on("function f() { return 1; }").is_empty());
    }

    #[test]
    fn flags_outer_when_only_inner_awaits() {
        // Outer has no await of its own; inner async fn does.
        let d = run_on("async function outer() { async function inner() { await x(); } }");
        assert_eq!(d.len(), 1);
    }
}
