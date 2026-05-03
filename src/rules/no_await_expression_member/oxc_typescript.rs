//! no-await-expression-member OXC backend — flag member access on `(await expr)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn unwrap_parens<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    let mut current = expr;
    while let Expression::ParenthesizedExpression(paren) = current {
        current = &paren.expression;
    }
    current
}

fn check_object_is_await(
    obj: &Expression,
    span_start: u32,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let inner = unwrap_parens(obj);
    if !matches!(inner, Expression::AwaitExpression(_)) {
        return;
    }

    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Do not access a member directly from an await expression \
                  — extract to a variable first."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::StaticMemberExpression,
            AstType::ComputedMemberExpression,
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
            AstKind::StaticMemberExpression(member) => {
                check_object_is_await(&member.object, member.span().start, ctx, diagnostics);
            }
            AstKind::ComputedMemberExpression(member) => {
                check_object_is_await(&member.object, member.span().start, ctx, diagnostics);
            }
            _ => {}
        }
    }
}
