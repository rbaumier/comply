//! prefer-early-return oxc backend — flag functions whose body is a single
//! `if` without `else`, with 2+ statements inside.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{FunctionBody, Statement};
use std::sync::Arc;

pub struct Check;

fn check_body(body: &FunctionBody, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    if body.statements.len() != 1 {
        return;
    }
    let Statement::IfStatement(if_stmt) = &body.statements[0] else {
        return;
    };
    // Must NOT have an else branch.
    if if_stmt.alternate.is_some() {
        return;
    }
    // Consequence must be a block with 2+ statements.
    let Statement::BlockStatement(block) = &if_stmt.consequent else {
        return;
    };
    if block.body.len() < 2 {
        return;
    }

    let (line, column) =
        byte_offset_to_line_col(ctx.source, if_stmt.span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Function body is wrapped in a single `if` — invert it as a guard clause with an early return.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Function(func) => {
                if let Some(ref body) = func.body {
                    check_body(body, ctx, diagnostics);
                }
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                check_body(&arrow.body, ctx, diagnostics);
            }
            _ => {}
        }
    }
}
