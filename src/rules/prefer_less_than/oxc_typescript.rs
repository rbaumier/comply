//! prefer-less-than oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

/// RHS expressions that indicate a variable-vs-literal comparison
/// (e.g. `x > 0`, `arr.length >= 1`) which should not be flagged.
fn is_literal_rhs(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::UnaryExpression(_)
    ) || matches!(expr, Expression::Identifier(id) if id.name.as_str() == "undefined")
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
        let AstKind::BinaryExpression(bin) = node.kind() else {
            return;
        };

        let suggested = match bin.operator {
            BinaryOperator::GreaterThan => "<",
            BinaryOperator::GreaterEqualThan => "<=",
            _ => return,
        };

        let op = match bin.operator {
            BinaryOperator::GreaterThan => ">",
            BinaryOperator::GreaterEqualThan => ">=",
            _ => return,
        };

        if is_literal_rhs(&bin.right) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `{suggested}` over `{op}` for readability — swap operands and use `{suggested}`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
