//! OxcCheck backend for prefer-single-boolean-return.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement, AstType::BlockStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::IfStatement(if_stmt) => {
                check_if_else(if_stmt, ctx, diagnostics);
            }
            AstKind::BlockStatement(block) => {
                check_sibling_return(block, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

fn check_if_else(
    if_stmt: &oxc_ast::ast::IfStatement,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(alt) = &if_stmt.alternate else {
        return;
    };

    // Skip `else if` — the alternative is itself an if_statement.
    if matches!(alt, Statement::IfStatement(_)) {
        return;
    }

    let Some(cons_bool) = extract_single_return_bool(&if_stmt.consequent) else {
        return;
    };
    let Some(alt_bool) = extract_single_return_bool(alt) else {
        return;
    };
    if cons_bool == alt_bool {
        return;
    }

    push_diag(if_stmt.span.start, ctx, diagnostics);
}

fn check_sibling_return(
    block: &oxc_ast::ast::BlockStatement,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let stmts = &block.body;
    for i in 0..stmts.len().saturating_sub(1) {
        let Statement::IfStatement(if_stmt) = &stmts[i] else {
            continue;
        };
        // Must have no `else` branch.
        if if_stmt.alternate.is_some() {
            continue;
        }
        let Some(first_bool) = extract_single_return_bool(&if_stmt.consequent) else {
            continue;
        };
        let Statement::ReturnStatement(ret) = &stmts[i + 1] else {
            continue;
        };
        let Some(second_bool) = return_bool_value(ret) else {
            continue;
        };
        if first_bool == second_bool {
            continue;
        }
        push_diag(if_stmt.span.start, ctx, diagnostics);
    }
}

fn push_diag(span_start: u32, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: "prefer-single-boolean-return".into(),
        message: "`if (cond) return <bool>; else return <bool>;` — return the condition directly."
            .into(),
        severity: Severity::Warning,
        span: Some((span_start as usize, 0)),
    });
}

fn extract_single_return_bool(stmt: &Statement) -> Option<bool> {
    match stmt {
        Statement::ReturnStatement(ret) => return_bool_value(ret),
        Statement::BlockStatement(block) => {
            let named: Vec<_> = block
                .body
                .iter()
                .filter(|s| !matches!(s, Statement::EmptyStatement(_)))
                .collect();
            if named.len() != 1 {
                return None;
            }
            if let Statement::ReturnStatement(ret) = named[0] {
                return return_bool_value(ret);
            }
            None
        }
        _ => None,
    }
}

fn return_bool_value(ret: &oxc_ast::ast::ReturnStatement) -> Option<bool> {
    let arg = ret.argument.as_ref()?;
    match arg {
        Expression::BooleanLiteral(b) => Some(b.value),
        _ => None,
    }
}
