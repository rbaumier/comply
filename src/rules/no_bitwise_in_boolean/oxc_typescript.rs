//! no-bitwise-in-boolean — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

const COMPARISON_OPS: &[BinaryOperator] = &[
    BinaryOperator::Equality,
    BinaryOperator::Inequality,
    BinaryOperator::StrictEquality,
    BinaryOperator::StrictInequality,
    BinaryOperator::LessThan,
    BinaryOperator::GreaterThan,
    BinaryOperator::LessEqualThan,
    BinaryOperator::GreaterEqualThan,
];

/// Check whether an expression contains a bitwise operator.
fn has_bitwise_op(expr: &Expression) -> bool {
    match expr {
        Expression::BinaryExpression(bin) => {
            if COMPARISON_OPS.contains(&bin.operator) {
                return false;
            }
            if matches!(
                bin.operator,
                BinaryOperator::BitwiseAnd
                    | BinaryOperator::BitwiseOR
                    | BinaryOperator::BitwiseXOR
            ) {
                return true;
            }
            has_bitwise_op(&bin.left) || has_bitwise_op(&bin.right)
        }
        Expression::UnaryExpression(un) => {
            if un.operator == UnaryOperator::BitwiseNot {
                return true;
            }
            false
        }
        Expression::ParenthesizedExpression(paren) => has_bitwise_op(&paren.expression),
        _ => false,
    }
}

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement, AstType::WhileStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (test, stmt_span) = match node.kind() {
            oxc_ast::AstKind::IfStatement(s) => (&s.test, s.span()),
            oxc_ast::AstKind::WhileStatement(s) => (&s.test, s.span()),
            _ => return,
        };

        if !has_bitwise_op(test) {
            return;
        }

        let (line, col) = byte_offset_to_line_col(semantic.source_text(), stmt_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: "Bitwise operator in boolean context — did you mean `&&` or `||`?".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
