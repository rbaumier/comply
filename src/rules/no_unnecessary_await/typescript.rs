//! no-unnecessary-await backend — flag `await` on non-promise values.
//!
//! Detects `await 42`, `await "hello"`, `await [1,2]`, `await () => {}`,
//! `await class {}`, etc. These are obviously not promises, so the `await`
//! is unnecessary noise.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns `true` if the node kind is obviously not a Promise.
fn is_not_promise(kind: &str) -> bool {
    matches!(
        kind,
        "array"
            | "arrow_function"
            | "await_expression"
            | "binary_expression"
            | "class"
            | "function_expression"  // tree-sitter uses "function" for function_expression
            | "function"
            | "jsx_element"
            | "jsx_self_closing_element"
            | "jsx_fragment"
            | "number"
            | "string"
            | "template_string"
            | "regex"
            | "true"
            | "false"
            | "null"
            | "undefined"
            | "unary_expression"
            | "update_expression"
    )
}

crate::ast_check! { on ["await_expression"] => |node, source, ctx, diagnostics|
    // The awaited value is the first named child of await_expression.
    let Some(argument) = node.named_child(0) else { return };

    // Unwrap parenthesized_expression layers.
    let mut unwrapped = argument;
    while unwrapped.kind() == "parenthesized_expression" {
        if let Some(child) = unwrapped.named_child(0) {
            unwrapped = child;
        } else {
            break;
        }
    }

    // For sequence_expression, check the last expression.
    let check_node = if unwrapped.kind() == "sequence_expression" {
        let count = unwrapped.named_child_count();
        if count == 0 { return; }
        match unwrapped.named_child(count - 1) {
            Some(last) => last,
            None => return,
        }
    } else {
        unwrapped
    };

    if !is_not_promise(check_node.kind()) {
        return;
    }

    let _ = source; // suppress unused warning

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-unnecessary-await".into(),
        message: "Do not `await` a non-promise value.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_await_number() {
        let d = run_on("async function f() { await 42; }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unnecessary-await");
    }

    #[test]
    fn flags_await_string() {
        let d = run_on("async function f() { await 'hello'; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_await_array() {
        let d = run_on("async function f() { await [1, 2, 3]; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_await_arrow_function() {
        let d = run_on("async function f() { await (() => {}); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_await_template_literal() {
        let d = run_on("async function f() { await `hello`; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_await_unary() {
        let d = run_on("async function f() { await !true; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_await_call() {
        assert!(run_on("async function f() { await fetch(url); }").is_empty());
    }

    #[test]
    fn allows_await_identifier() {
        assert!(run_on("async function f() { await promise; }").is_empty());
    }

    #[test]
    fn allows_await_new_promise() {
        assert!(run_on("async function f() { await new Promise(r => r()); }").is_empty());
    }
}
