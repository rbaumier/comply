//! prefer-mock-return-shorthand backend — flag
//! `x.mockImplementation(() => value)` where the arrow/function body just
//! returns a value, suggest `x.mockReturnValue(value)`.
//!
//! Why: Jest / Vitest provide `.mockReturnValue(v)` as a shorter, clearer
//! alternative to wrapping a static value in an arrow function. We sidestep
//! `Promise.resolve/reject` bodies (handled by `prefer-mock-promise-shorthand`).

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// Return true if `expr` is `Promise.resolve(...)` or `Promise.reject(...)`.
/// These are covered by `prefer-mock-promise-shorthand` and must not be
/// double-flagged here.
fn is_promise_settle<'a>(expr: Node<'a>, source: &'a [u8]) -> bool {
    if expr.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = expr.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(object) = callee.child_by_field_name("object") else {
        return false;
    };
    let Some(property) = callee.child_by_field_name("property") else {
        return false;
    };
    if object.utf8_text(source).ok() != Some("Promise") {
        return false;
    }
    matches!(
        property.utf8_text(source).ok(),
        Some("resolve") | Some("reject")
    )
}

/// Extract the returned expression of a function whose body is either a single
/// expression (arrow concise body) or a block with a single `return`.
/// Returns None if the body is more complex.
fn single_return_expr<'a>(func: Node<'a>) -> Option<Node<'a>> {
    let body = func.child_by_field_name("body")?;

    // Arrow function concise body: body IS the expression.
    if body.kind() != "statement_block" {
        return Some(body);
    }

    // Block body: must contain exactly one return statement that returns an
    // expression. Comments are allowed; anything else rejects the match.
    let mut return_stmt: Option<Node> = None;
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        if return_stmt.is_some() {
            return None;
        }
        if child.kind() != "return_statement" {
            return None;
        }
        return_stmt = Some(child);
    }
    let ret = return_stmt?;
    ret.named_child(0)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }

    // Callee must be a member access ending in `.mockImplementation`.
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let Some(property) = callee.child_by_field_name("property") else { return; };
    if property.utf8_text(source).ok() != Some("mockImplementation") { return; }

    // Exactly one argument: an arrow/function whose body reduces to a single
    // returned expression.
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    if args.named_child_count() != 1 { return; }
    let Some(arg) = args.named_child(0) else { return; };
    if arg.kind() != "arrow_function" && arg.kind() != "function_expression" && arg.kind() != "function" {
        return;
    }

    let Some(expr) = single_return_expr(arg) else { return; };

    // Delegate Promise.resolve/reject bodies to `prefer-mock-promise-shorthand`.
    if is_promise_settle(expr, source) { return; }

    let pos = property.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-mock-return-shorthand".into(),
        message:
            "Prefer `.mockReturnValue(x)` over `.mockImplementation(() => x)`."
                .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    // ---- flags violations ----

    #[test]
    fn flags_arrow_expression_body_literal() {
        assert_eq!(run("fn.mockImplementation(() => 42);").len(), 1);
    }

    #[test]
    fn flags_arrow_block_body_single_return() {
        assert_eq!(
            run("fn.mockImplementation(() => { return 42; });").len(),
            1
        );
    }

    #[test]
    fn flags_function_expression_single_return() {
        assert_eq!(
            run("fn.mockImplementation(function () { return value; });").len(),
            1
        );
    }

    #[test]
    fn flags_arrow_returning_object() {
        assert_eq!(
            run("fn.mockImplementation(() => ({ id: 1 }));").len(),
            1
        );
    }

    // ---- allows correct usage ----

    #[test]
    fn allows_mock_return_value_shorthand() {
        assert!(run("fn.mockReturnValue(42);").is_empty());
    }

    #[test]
    fn allows_implementation_with_logic() {
        let src = "fn.mockImplementation(() => { doWork(); return 1; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_implementation_with_multiple_statements() {
        let src =
            "fn.mockImplementation(() => { const x = compute(); return x + 1; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_promise_resolve_body() {
        // Covered by `prefer-mock-promise-shorthand`; must not double-report.
        assert!(run("fn.mockImplementation(() => Promise.resolve(1));").is_empty());
    }

    #[test]
    fn skips_promise_reject_body() {
        assert!(
            run("fn.mockImplementation(() => Promise.reject(new Error('x')));")
                .is_empty()
        );
    }

    #[test]
    fn allows_unrelated_call() {
        assert!(run("foo(() => 42);").is_empty());
    }
}
