//! no-one-iteration-loop OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

fn is_unconditional_exit(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::ReturnStatement(_) | Statement::BreakStatement(_) | Statement::ThrowStatement(_)
    )
}

/// Detect a `continue` that targets THIS loop. Unlabeled `continue` targets
/// the innermost loop, so nested loops are pruned — their `continue` is theirs.
/// Labeled `continue` is handled separately by `contains_labeled_continue`,
/// since its label can name an enclosing loop and so escape any nesting.
fn contains_continue(stmt: &Statement) -> bool {
    match stmt {
        Statement::ContinueStatement(_) => true,
        // Don't descend into nested loops
        Statement::ForStatement(_)
        | Statement::ForInStatement(_)
        | Statement::ForOfStatement(_)
        | Statement::WhileStatement(_)
        | Statement::DoWhileStatement(_) => false,
        Statement::IfStatement(if_stmt) => {
            contains_continue(&if_stmt.consequent)
                || if_stmt
                    .alternate
                    .as_ref()
                    .is_some_and(|a| contains_continue(a))
        }
        Statement::BlockStatement(block) => {
            block.body.iter().any(|s| contains_continue(s))
        }
        Statement::LabeledStatement(l) => contains_continue(&l.body),
        Statement::TryStatement(t) => {
            t.block.body.iter().any(|s| contains_continue(s))
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.body.iter().any(|s| contains_continue(s)))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| f.body.iter().any(|s| contains_continue(s)))
        }
        _ => false,
    }
}

/// Detect any labeled `continue <label>;` anywhere in `stmt`, descending
/// through nested loops as well. A labeled continue can name an enclosing
/// loop, so it escapes the inner loop and keeps that outer loop iterating —
/// the outer loop is then not redundant. We can't cheaply prove the label
/// names *this* loop, so conservatively treat any labeled continue as a reason
/// to bail.
fn contains_labeled_continue(stmt: &Statement) -> bool {
    match stmt {
        Statement::ContinueStatement(c) => c.label.is_some(),
        Statement::ForStatement(s) => contains_labeled_continue(&s.body),
        Statement::ForInStatement(s) => contains_labeled_continue(&s.body),
        Statement::ForOfStatement(s) => contains_labeled_continue(&s.body),
        Statement::WhileStatement(s) => contains_labeled_continue(&s.body),
        Statement::DoWhileStatement(s) => contains_labeled_continue(&s.body),
        Statement::IfStatement(if_stmt) => {
            contains_labeled_continue(&if_stmt.consequent)
                || if_stmt
                    .alternate
                    .as_ref()
                    .is_some_and(|a| contains_labeled_continue(a))
        }
        Statement::BlockStatement(block) => {
            block.body.iter().any(contains_labeled_continue)
        }
        Statement::LabeledStatement(l) => contains_labeled_continue(&l.body),
        Statement::SwitchStatement(sw) => sw
            .cases
            .iter()
            .any(|case| case.consequent.iter().any(contains_labeled_continue)),
        Statement::TryStatement(t) => {
            t.block.body.iter().any(contains_labeled_continue)
                || t.handler.as_ref().is_some_and(|h| {
                    h.body.body.iter().any(contains_labeled_continue)
                })
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| f.body.iter().any(contains_labeled_continue))
        }
        _ => false,
    }
}

fn check_loop_body(
    body: &Statement,
    loop_span: oxc_span::Span,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Statement::BlockStatement(block) = body else {
        return;
    };
    let stmts = &block.body;
    let Some(last) = stmts.last() else {
        return;
    };
    if !is_unconditional_exit(last) {
        return;
    }
    // If any earlier statement contains a `continue` that targets this loop —
    // either an unlabeled `continue` at this level, or a labeled `continue`
    // anywhere (including inside a nested loop, where its label can name this
    // outer loop) — the loop may iterate more than once. Bail.
    for s in &stmts[..stmts.len().saturating_sub(1)] {
        if contains_continue(s) || contains_labeled_continue(s) {
            return;
        }
    }

    let (line, column) = byte_offset_to_line_col(ctx.source, loop_span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Loop body always exits on the first iteration — the loop is redundant.".into(),
        severity: Severity::Warning,
        span: Some((loop_span.start as usize, loop_span.size() as usize)),
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ForStatement,
            AstType::ForInStatement,
            AstType::WhileStatement,
            AstType::DoWhileStatement,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ForStatement(stmt) => {
                check_loop_body(&stmt.body, stmt.span, ctx, diagnostics);
            }
            AstKind::ForInStatement(stmt) => {
                check_loop_body(&stmt.body, stmt.span, ctx, diagnostics);
            }
            AstKind::WhileStatement(stmt) => {
                check_loop_body(&stmt.body, stmt.span, ctx, diagnostics);
            }
            AstKind::DoWhileStatement(stmt) => {
                check_loop_body(&stmt.body, stmt.span, ctx, diagnostics);
            }
            _ => {}
        }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_for_with_unconditional_return() {
        let src = r#"function f() {
    for (let i = 0; i < 10; i++) {
        doWork();
        return;
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_while_with_unconditional_break() {
        let src = r#"function f() {
    while (true) {
        doWork();
        break;
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_for_in_with_unconditional_throw() {
        let src = r#"function f(obj: Record<string, unknown>) {
    for (const k in obj) {
        throw new Error(k);
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_plain_while_with_return() {
        let src = r#"function f(cond: boolean) {
    while (cond) {
        return 1;
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_loop_with_conditional_break() {
        let src = r#"function f() {
    for (let i = 0; i < 10; i++) {
        if (cond(i)) break;
        doWork(i);
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_loop_with_continue() {
        let src = r#"function f() {
    for (let i = 0; i < 10; i++) {
        if (i === 0) continue;
        return;
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_normal_loop() {
        let src = r#"function f() {
    for (let i = 0; i < 10; i++) {
        doWork(i);
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    /// Regression for #3946 — a labeled `continue` inside a nested `for…of`
    /// targets the OUTER labeled `while`, so the outer loop can iterate many
    /// times and is not redundant. Must not flag.
    #[test]
    fn no_fp_labeled_continue_from_nested_loop() {
        let src = r#"function f(cond: boolean, xs: number[], t: boolean) {
    outer: while (cond) {
        for (const x of xs) {
            if (t) {
                continue outer;
            }
        }
        throw new Error("x");
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    /// An UNLABELED `continue` inside a nested loop targets that nested loop —
    /// it does NOT escape to the outer loop, so the outer `while` still runs
    /// exactly once and must STILL flag.
    #[test]
    fn flags_unlabeled_continue_in_nested_loop() {
        let src = r#"function f(cond: boolean, xs: number[], t: boolean) {
    while (cond) {
        for (const x of xs) {
            if (t) {
                continue;
            }
        }
        throw new Error("x");
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
