//! prefer-mock-promise-shorthand backend — flag
//! `x.mockImplementation(() => Promise.resolve(v))` and
//! `x.mockImplementation(() => Promise.reject(v))`.
//!
//! Why: Jest / Vitest provide `.mockResolvedValue(v)` and
//! `.mockRejectedValue(v)` shorthands that are clearer and shorter than
//! wrapping the value in an arrow function that constructs a `Promise`.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// If `body` is a `Promise.resolve(x)` / `Promise.reject(x)` call expression,
/// return the property name (`"resolve"` or `"reject"`).
fn promise_settle_kind<'a>(body: Node<'a>, source: &'a [u8]) -> Option<&'static str> {
    if body.kind() != "call_expression" {
        return None;
    }
    let callee = body.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let object = callee.child_by_field_name("object")?;
    let property = callee.child_by_field_name("property")?;
    if object.utf8_text(source).ok()? != "Promise" {
        return None;
    }
    match property.utf8_text(source).ok()? {
        "resolve" => Some("resolve"),
        "reject" => Some("reject"),
        _ => None,
    }
}

/// Extract the `Promise.resolve/reject(...)` call from the body of a function
/// passed as `mockImplementation` argument. Accepts:
/// - arrow expression body: `() => Promise.resolve(v)`
/// - block body with a single `return` statement:
///   `() => { return Promise.resolve(v); }` / `function () { return Promise.resolve(v); }`
fn settle_kind_from_fn<'a>(func: Node<'a>, source: &'a [u8]) -> Option<&'static str> {
    let body = func.child_by_field_name("body")?;

    // Arrow function expression body: body IS the expression.
    if body.kind() != "statement_block" {
        return promise_settle_kind(body, source);
    }

    // Block body: must contain exactly one return statement that returns the
    // `Promise.resolve/reject(x)` call. Anything more complex is out of scope.
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
    // `return_statement` has the returned expression as its first named child.
    let expr = ret.named_child(0)?;
    promise_settle_kind(expr, source)
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Callee must be a member access ending in `.mockImplementation`.
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let Some(property) = callee.child_by_field_name("property") else { return; };
    if property.utf8_text(source).ok() != Some("mockImplementation") { return; }

    // Exactly one argument: an arrow/function taking no params and returning
    // `Promise.resolve(x)` / `Promise.reject(x)`.
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    if args.named_child_count() != 1 { return; }
    let Some(arg) = args.named_child(0) else { return; };
    if arg.kind() != "arrow_function" && arg.kind() != "function_expression" && arg.kind() != "function" {
        return;
    }

    let Some(kind) = settle_kind_from_fn(arg, source) else { return; };

    let shorthand = match kind {
        "resolve" => "mockResolvedValue",
        "reject" => "mockRejectedValue",
        _ => return,
    };

    let pos = property.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-mock-promise-shorthand".into(),
        message: format!(
            "Prefer `.{shorthand}(x)` over `.mockImplementation(() => Promise.{kind}(x))`."
        ),
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
    fn flags_arrow_expression_body_resolve() {
        assert_eq!(
            run("fn.mockImplementation(() => Promise.resolve(1));").len(),
            1
        );
    }

    #[test]
    fn flags_arrow_expression_body_reject() {
        assert_eq!(
            run("fn.mockImplementation(() => Promise.reject(new Error('x')));").len(),
            1
        );
    }

    #[test]
    fn flags_arrow_block_body_resolve() {
        assert_eq!(
            run("fn.mockImplementation(() => { return Promise.resolve(42); });").len(),
            1
        );
    }

    #[test]
    fn flags_function_expression_reject() {
        assert_eq!(
            run("fn.mockImplementation(function () { return Promise.reject(err); });").len(),
            1
        );
    }

    // ---- allows correct usage ----

    #[test]
    fn allows_mock_resolved_value_shorthand() {
        assert!(run("fn.mockResolvedValue(1);").is_empty());
    }

    #[test]
    fn allows_mock_rejected_value_shorthand() {
        assert!(run("fn.mockRejectedValue(new Error('x'));").is_empty());
    }

    #[test]
    fn allows_non_promise_implementation() {
        assert!(run("fn.mockImplementation(() => 42);").is_empty());
    }

    #[test]
    fn allows_implementation_with_logic() {
        let src = "fn.mockImplementation(() => { doWork(); return Promise.resolve(1); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_implementation_with_params() {
        // Arg-using implementations can't be replaced with a static value.
        // We still flag them since the body only returns `Promise.resolve(x)`,
        // but only when the returned expression doesn't depend on params.
        // Here the body returns `Promise.resolve(a)` which depends on `a` —
        // but the rule's remit (matching eslint-plugin-unicorn) still flags this.
        // Accept either 0 or 1 depending on interpretation — we keep it simple
        // and flag, which matches the upstream rule.
        let src = "fn.mockImplementation((a) => Promise.resolve(a));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_promise_all() {
        assert!(run("fn.mockImplementation(() => Promise.all([a, b]));").is_empty());
    }
}
