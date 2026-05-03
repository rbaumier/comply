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
    // If any earlier statement contains a `continue`, bail.
    for s in &stmts[..stmts.len().saturating_sub(1)] {
        if contains_continue(s) {
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
