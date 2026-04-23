//! prefer-early-return backend.
//!
//! A function body that is a single `if (cond) { ...substantial body... }`
//! (no `else`) can always be rewritten as `if (!cond) return; ...body...`.
//! The guard clause reduces indentation and keeps the happy path at the
//! outer scope.
//!
//! Triggers when:
//!   - A function's `statement_block` body contains exactly one named
//!     child which is an `if_statement` without `alternative`.
//!   - The `if` body is a `statement_block` with 2+ statements (inverting
//!     a single-statement body is noise, not improvement).

use crate::diagnostic::{Diagnostic, Severity};

const FUNC_KINDS: &[&str] = &[
    "function_declaration",
    "function",
    "function_expression",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if !FUNC_KINDS.contains(&node.kind()) {
        return;
    }
    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" {
        return;
    }

    let mut cursor = body.walk();
    let stmts: Vec<_> = body.named_children(&mut cursor).collect();
    if stmts.len() != 1 {
        return;
    }
    let only = stmts[0];
    if only.kind() != "if_statement" {
        return;
    }
    // Must NOT have an else branch — otherwise the guard rewrite isn't
    // trivially equivalent.
    if only.child_by_field_name("alternative").is_some() {
        return;
    }
    let Some(cons) = only.child_by_field_name("consequence") else { return };
    if cons.kind() != "statement_block" {
        return;
    }
    // Require at least 2 statements inside the if — inverting a
    // one-liner is churn, not improvement.
    let mut cc = cons.walk();
    let inner: Vec<_> = cons.named_children(&mut cc).collect();
    if inner.len() < 2 {
        return;
    }

    let pos = only.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-early-return".into(),
        message: "Function body is wrapped in a single `if` — invert it as a guard clause with an early return.".into(),
        severity: Severity::Warning,
        span: Some((only.byte_range().start, only.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_single_if_wrapping_body() {
        let src = r#"function f(x: number) {
    if (x > 0) {
        doA();
        doB();
        doC();
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_arrow_function() {
        let src = r#"const f = (x: number) => {
    if (x > 0) {
        doA();
        doB();
    }
};"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_method() {
        let src = r#"class C {
    m(x: number) {
        if (x > 0) {
            doA();
            doB();
        }
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_if_with_else() {
        let src = r#"function f(x: number) {
    if (x > 0) {
        doA();
        doB();
    } else {
        doC();
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_line_if_body() {
        let src = r#"function f(x: number) {
    if (x > 0) {
        doA();
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_multiple_statements_in_function() {
        let src = r#"function f(x: number) {
    const y = x * 2;
    if (y > 0) {
        doA();
        doB();
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_else_if_chain() {
        let src = r#"function f(x: number) {
    if (x > 0) {
        doA();
        doB();
    } else if (x < 0) {
        doC();
        doD();
    }
}"#;
        assert!(run_on(src).is_empty());
    }
}
