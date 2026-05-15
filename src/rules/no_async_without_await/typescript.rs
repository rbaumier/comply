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

fn is_test_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("/tests/")
        || s.contains("\\tests\\")
}

fn is_async_function(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.utf8_text(source).unwrap_or("") == "async" {
            return true;
        }
    }
    false
}

fn has_promise_return_type(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.child_by_field_name("return_type")
        .and_then(|return_type| return_type.utf8_text(source).ok())
        .is_some_and(|text| text.contains("Promise<") || text.contains("PromiseLike<"))
}

fn has_decorator_child(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| child.kind() == "decorator")
}

fn method_has_decorator(method: tree_sitter::Node) -> bool {
    if has_decorator_child(method) {
        return true;
    }
    let Some(parent) = method.parent() else {
        return false;
    };
    let mut cursor = parent.walk();
    let mut decorator_before_current = false;
    for child in parent.named_children(&mut cursor) {
        if child.kind() == "decorator" {
            decorator_before_current = true;
            continue;
        }
        if child.start_byte() == method.start_byte() && child.end_byte() == method.end_byte() {
            return decorator_before_current;
        }
        decorator_before_current = false;
    }
    false
}

fn method_is_in_decorated_class(method: tree_sitter::Node) -> bool {
    if method.kind() != "method_definition" {
        return false;
    }
    let Some(class_body) = method.parent() else {
        return false;
    };
    if class_body.kind() != "class_body" {
        return false;
    }
    let Some(class_node) = class_body.parent() else {
        return false;
    };
    if !matches!(class_node.kind(), "class_declaration" | "class") {
        return false;
    }
    if has_decorator_child(class_node) {
        return true;
    }
    class_node.parent().is_some_and(has_decorator_child)
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
        if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
            return;
        }
        if !is_async_function(node, source) {
            return;
        }
        if has_promise_return_type(node, source) {
            return;
        }
        if method_has_decorator(node) || method_is_in_decorated_class(node) {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        if body_has_await(body, source) {
            return;
        }
        if let Ok(text) = body.utf8_text(source) {
            if text.contains("Result.await") || text.contains("Result.gen") {
                return;
            }
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
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

    #[test]
    fn allows_async_function_with_explicit_promise_contract() {
        let src = "async function handler(): Promise<void> { return; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_method_in_decorated_class() {
        let src = "@Controller()\nclass C { async onModuleInit(): Promise<void> { return; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_decorated_async_method() {
        let src = "class C { @GrpcMethod('Math') async sum() { return 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_await_pattern() {
        let src = r#"const run = async () => { return Result.gen(async function* () { const v = yield* Result.await(fetch()); return v; }); };"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_gen_pattern() {
        let src = r#"async function handler() { return Result.gen(async function* () { yield* Result.await(doStuff()); }); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_without_await_in_test_file() {
        let src = "it('works', async () => { return request(server).expect(200); });";
        let d = crate::rules::test_helpers::run_ts_with_path(src, &Check, "handler.test.ts");
        assert!(d.is_empty());
    }
}
