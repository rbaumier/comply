//! Flags `union_type`s in return-type position that include a
//! `Promise<...>` alternative alongside a non-Promise alternative.

use crate::diagnostic::{Diagnostic, Severity};

fn is_promise_type(node: tree_sitter::Node, source: &[u8]) -> bool {
    // generic_type with name "Promise"
    if node.kind() == "generic_type"
        && let Some(name) = node.child_by_field_name("name")
    {
        let text = std::str::from_utf8(&source[name.byte_range()]).unwrap_or("");
        return text == "Promise";
    }
    false
}

fn is_return_type_position(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    // return_type_annotation or type_annotation as return type
    parent.kind() == "type_annotation" || parent.kind() == "return_type"
}

crate::ast_check! { on ["union_type", "function_declaration", "function_expression", "arrow_function", "method_definition"] => |node, source, ctx, diagnostics|
match node.kind() {
        "union_type" => check_annotated_union(node, source, ctx, diagnostics),
        "function_declaration" | "function_expression" | "arrow_function" | "method_definition" => {
            check_function_body(node, source, ctx, diagnostics);
        }
        _ => {}
    }
}

fn check_annotated_union(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Walk up through parentheses / type_annotation to check we're in a return type.
    let mut probe = node;
    let mut in_return_type = false;
    for _ in 0..4 {
        let Some(parent) = probe.parent() else { break };
        let pk = parent.kind();
        if pk == "function_signature"
            || pk == "function_declaration"
            || pk == "method_signature"
            || pk == "method_definition"
            || pk == "arrow_function"
            || pk == "function_expression"
            || pk == "function_type"
        {
            // Only flag if we're the return_type child, not a parameter type.
            if let Some(ret) = parent.child_by_field_name("return_type")
                && ret.id() == probe.id()
            {
                in_return_type = true;
            }
            break;
        }
        if !is_return_type_position(probe) && pk != "parenthesized_type" {
            // continue walking up through parens/annotations
        }
        probe = parent;
    }

    if !in_return_type {
        return;
    }

    let mut cursor = node.walk();
    let members: Vec<_> = node.named_children(&mut cursor).collect();
    let has_promise = members.iter().any(|m| is_promise_type(*m, source));
    let has_non_promise = members.iter().any(|m| !is_promise_type(*m, source));

    if has_promise && has_non_promise {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Return type mixes sync and Promise values; mark the function `async` so it always returns a Promise.".into(),
            Severity::Warning,
        ));
    }
}

/// Scan a non-async function body for both a plain `return <expr>` (where the
/// expression is clearly synchronous, e.g. a literal or identifier) and a
/// promise-returning return (`return new Promise(...)`, `return someAsync()`,
/// or `return await ...`). When both occur in the same function, flag.
fn check_function_body(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if function_is_async(node, source) {
        return;
    }
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    if body.kind() != "statement_block" {
        return;
    }

    let mut has_sync_value = false;
    let mut has_async_value = false;
    collect_returns(body, source, &mut has_sync_value, &mut has_async_value);
    if has_sync_value && has_async_value {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Function returns both a sync value and a Promise; mark it `async` so callers always `await`.".into(),
            Severity::Warning,
        ));
    }
}

fn function_is_async(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    // tree-sitter-typescript exposes `async` as an anonymous keyword child;
    // a textual scan of the leading signature is the cheapest way to detect it.
    let Some(body) = node.child_by_field_name("body") else {
        return false;
    };
    let head = &source[node.start_byte()..body.start_byte()];
    std::str::from_utf8(head)
        .map(|t| t.contains("async"))
        .unwrap_or(false)
}

fn collect_returns(
    n: tree_sitter::Node<'_>,
    source: &[u8],
    has_sync: &mut bool,
    has_async: &mut bool,
) {
    if matches!(
        n.kind(),
        "function_declaration" | "function_expression" | "arrow_function" | "method_definition"
    ) {
        return; // don't descend into nested functions
    }
    if n.kind() == "return_statement"
        && let Some(expr) = n.named_child(0)
    {
        match classify_return_expr(expr, source) {
            ReturnKind::Sync => *has_sync = true,
            ReturnKind::Async => *has_async = true,
            ReturnKind::Unknown => {}
        }
    }
    let mut cursor = n.walk();
    for c in n.children(&mut cursor) {
        collect_returns(c, source, has_sync, has_async);
    }
}

enum ReturnKind {
    Sync,
    Async,
    Unknown,
}

fn classify_return_expr(expr: tree_sitter::Node<'_>, source: &[u8]) -> ReturnKind {
    match expr.kind() {
        // `return await ...` — async.
        "await_expression" => ReturnKind::Async,
        // `return new Promise(...)`.
        "new_expression" => {
            let ctor_text = expr
                .child_by_field_name("constructor")
                .and_then(|c| c.utf8_text(source).ok())
                .unwrap_or("");
            if ctor_text == "Promise" {
                ReturnKind::Async
            } else {
                ReturnKind::Sync
            }
        }
        // `return Promise.resolve(...)` / `return Promise.reject(...)`.
        "call_expression" => {
            let callee_text = expr
                .child_by_field_name("function")
                .and_then(|c| c.utf8_text(source).ok())
                .unwrap_or("");
            if callee_text.starts_with("Promise.") {
                return ReturnKind::Async;
            }
            // Other call expressions — unknown (the callee may or may not be
            // async). Stay conservative.
            ReturnKind::Unknown
        }
        // Literals / identifiers / object/array — sync values.
        "string" | "number" | "true" | "false" | "null" | "undefined" | "identifier" | "object"
        | "array" | "template_string" => ReturnKind::Sync,
        _ => ReturnKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_mixed_return_type() {
        let src = "function f(): string | Promise<string> { return 'x'; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_mixed_method_signature() {
        let src = "interface I { run(): number | Promise<number>; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_pure_promise_return() {
        let src = "function f(): Promise<string> { return Promise.resolve('x'); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_union_in_parameter() {
        let src = "function f(x: string | Promise<string>): void {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_unannotated_mixed_returns_new_promise() {
        let src = "function f(x) { if (x) { return new Promise((r) => r(1)); } return 1; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unannotated_mixed_returns_promise_resolve() {
        let src = "function f(x) { if (x) { return Promise.resolve(1); } return 2; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_async_function_with_mixed_returns() {
        // Async functions always wrap in a Promise; this is fine.
        let src = "async function f(x) { if (x) { return Promise.resolve(1); } return 2; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_only_sync_returns() {
        let src = "function f(x) { if (x) { return 1; } return 2; }";
        assert!(run(src).is_empty());
    }
}
