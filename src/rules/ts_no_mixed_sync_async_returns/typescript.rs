//! Flags a non-async function whose body returns both a synchronous value and a
//! Promise. An explicit `T | Promise<T>` return-type annotation is an
//! intentional dual-mode contract (a handler returns sync when it can, async
//! only when it must) and is not flagged.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["function_declaration", "function_expression", "arrow_function", "method_definition"] => |node, source, ctx, diagnostics|
match node.kind() {
        "function_declaration" | "function_expression" | "arrow_function" | "method_definition" => {
            check_function_body(node, source, ctx, diagnostics);
        }
        _ => {}
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn allows_explicit_dual_mode_return_type() {
        // Explicit `T | Promise<T>` return annotation is an intentional dual-mode
        // contract; the body returns only a sync value (issue #3779).
        let src = "function f(): string | Promise<string> { return 'x'; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_explicit_dual_mode_method_signature() {
        let src = "interface I { run(): number | Promise<number>; }";
        assert!(run(src).is_empty());
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
