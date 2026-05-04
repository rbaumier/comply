//! operation-returning-nan oxc backend — flag arithmetic that produces NaN.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

/// Returns true if the expression is a string literal or template literal.
fn is_string(expr: &Expression) -> bool {
    matches!(expr, Expression::StringLiteral(_) | Expression::TemplateLiteral(_))
}

/// Returns true if the expression is `undefined`.
fn is_undefined(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(id) if id.name.as_str() == "undefined")
}

/// Returns true if the operator is arithmetic (not `+` which is also string concat).
fn is_arith_op(op: BinaryOperator) -> bool {
    matches!(
        op,
        BinaryOperator::Subtraction
            | BinaryOperator::Multiplication
            | BinaryOperator::Division
            | BinaryOperator::Remainder
            | BinaryOperator::Exponential
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        let has_undefined = is_undefined(&bin.left) || is_undefined(&bin.right);
        if has_undefined && (bin.operator == BinaryOperator::Addition || is_arith_op(bin.operator)) {
            let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Arithmetic with `undefined` will produce `NaN`.".into(),
                severity: Severity::Error,
                span: None,
            });
            return;
        }

        let has_string = is_string(&bin.left) || is_string(&bin.right);
        if has_string && is_arith_op(bin.operator) {
            let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Arithmetic on a string literal will produce `NaN`.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
