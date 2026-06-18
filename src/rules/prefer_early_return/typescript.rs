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
//!   - The `if` body is a `statement_block` with at least `min_statements`
//!     statements (inverting a small body is noise, not improvement).

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
    // Require at least `min_statements` inside the if — inverting a small
    // positive `if` is churn, not improvement (see #4067).
    let mut cc = cons.walk();
    let inner: Vec<_> = cons.named_children(&mut cc).collect();
    let min_statements = ctx
        .config
        .threshold("prefer-early-return", "min_statements", ctx.lang);
    if inner.len() < min_statements {
        return;
    }

    let pos = only.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-early-return".into(),
        message: "Function body is wrapped in a single `if` — invert it as a guard clause with an early return.".into(),
        severity: Severity::Warning,
        span: Some((only.byte_range().start, only.byte_range().len())),
    });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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
        doC();
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
            doC();
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
    fn allows_two_statement_body() {
        // Regression for #4067: a function whose entire body is a single `if`
        // wrapping only two statements reads fine as a positive condition;
        // inverting it into `if (!cond) return;` is not a readability win.
        let src = r#"function reportIfServerError(args: Args): void {
    if (args.status >= 500) {
        const reportable = pick(args);
        args.reporter.captureException(reportable);
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
