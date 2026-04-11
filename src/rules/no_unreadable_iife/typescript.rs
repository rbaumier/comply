//! no-unreadable-iife backend — flag IIFEs whose arrow function body is
//! wrapped in parentheses, making them hard to read.
//!
//! The original unicorn rule flags:
//!   `const foo = (() => (bar))();`
//! where the arrow body `(bar)` is parenthesized — the outer `()` invocation
//! makes it unclear whether the parens around `bar` are for grouping or
//! for the IIFE call.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node is wrapped in parentheses by looking for
/// `parenthesized_expression` as the body of an arrow function.
fn is_parenthesized_body(body: tree_sitter::Node) -> bool {
    body.kind() == "parenthesized_expression"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    // The callee of the call expression must be an arrow function.
    let Some(callee) = node.child_by_field_name("function") else { return };

    // The callee might be wrapped in parentheses: `(() => (x))()`
    // Unwrap parenthesized_expression layers to find the arrow function.
    let mut inner = callee;
    while inner.kind() == "parenthesized_expression" {
        if let Some(child) = inner.named_child(0) {
            inner = child;
        } else {
            break;
        }
    }

    if inner.kind() != "arrow_function" {
        return;
    }

    // The arrow function body must NOT be a block statement (that's a
    // normal multi-statement IIFE), and it MUST be parenthesized.
    let Some(body) = inner.child_by_field_name("body") else { return };

    if body.kind() == "statement_block" {
        return;
    }

    if !is_parenthesized_body(body) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-unreadable-iife".into(),
        message: "IIFE with parenthesized arrow function body is considered unreadable.".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_parenthesized_arrow_iife() {
        let d = run_on("const foo = (() => (bar))();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unreadable-iife");
    }

    #[test]
    fn flags_multiline_parenthesized_arrow_iife() {
        let d = run_on("const foo = (() => (bar + baz))();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_arrow_iife_without_parens_body() {
        // `(() => bar)()` — body is not parenthesized, fine.
        assert!(run_on("const foo = (() => bar)();").is_empty());
    }

    #[test]
    fn allows_arrow_iife_with_block_body() {
        // `(() => { return bar; })()` — block body, fine.
        assert!(run_on("const foo = (() => { return bar; })();").is_empty());
    }

    #[test]
    fn allows_regular_function_iife() {
        // `(function() { return 42; })()` — not an arrow function, fine.
        assert!(run_on("(function() { return 42; })();").is_empty());
    }

    #[test]
    fn allows_normal_call() {
        assert!(run_on("foo(bar);").is_empty());
    }
}
