//! OXC backend for prefer-math-trunc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_zero_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::NumericLiteral(lit) if lit.value == 0.0)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::UnaryExpression,
            AstType::BinaryExpression,
            AstType::AssignmentExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::UnaryExpression(unary) => {
                // ~~x: outer ~ whose argument is also ~expr
                if unary.operator != UnaryOperator::BitwiseNot {
                    return;
                }
                let Expression::UnaryExpression(inner) = &unary.argument else {
                    return;
                };
                if inner.operator != UnaryOperator::BitwiseNot {
                    return;
                }
                // Don't double-fire: skip if our parent is also `~`
                let parent = semantic.nodes().parent_node(node.id());
                if let AstKind::UnaryExpression(p) = parent.kind()
                    && p.operator == UnaryOperator::BitwiseNot {
                        return;
                    }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, unary.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Use `Math.trunc(x)` instead of `~~x`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::BinaryExpression(bin) => {
                let op = bin.operator;
                if !matches!(
                    op,
                    BinaryOperator::BitwiseOR
                        | BinaryOperator::ShiftRight
                        | BinaryOperator::ShiftLeft
                        | BinaryOperator::BitwiseXOR
                ) {
                    return;
                }
                if !is_zero_literal(&bin.right) {
                    return;
                }
                let op_str = &ctx.source[bin.left.span().end as usize..bin.right.span().start as usize].trim();
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, bin.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Use `Math.trunc(x)` instead of bitwise `{op_str} 0`."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::AssignmentExpression(assign) => {
                use oxc_ast::ast::AssignmentOperator;
                if !matches!(
                    assign.operator,
                    AssignmentOperator::BitwiseOR
                        | AssignmentOperator::ShiftRight
                        | AssignmentOperator::ShiftLeft
                        | AssignmentOperator::BitwiseXOR
                ) {
                    return;
                }
                if !is_zero_literal(&assign.right) {
                    return;
                }
                let op_str = match assign.operator {
                    AssignmentOperator::BitwiseOR => "|=",
                    AssignmentOperator::ShiftRight => ">>=",
                    AssignmentOperator::ShiftLeft => "<<=",
                    AssignmentOperator::BitwiseXOR => "^=",
                    _ => return,
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Use `Math.trunc(x)` instead of bitwise assignment `{op_str} 0`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
