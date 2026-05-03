//! no-collection-size-mischeck OXC backend — flag `.length >= 0` (always true)
//! and `.length < 0` (always false).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

fn is_length_or_size(expr: &Expression) -> bool {
    match expr {
        Expression::StaticMemberExpression(m) => {
            let name = m.property.name.as_str();
            name == "length" || name == "size"
        }
        _ => false,
    }
}

fn is_zero(expr: &Expression) -> bool {
    matches!(expr, Expression::NumericLiteral(lit) if lit.value == 0.0)
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

        let op = bin.operator;
        if op != BinaryOperator::GreaterEqualThan && op != BinaryOperator::LessThan {
            return;
        }

        if !is_length_or_size(&bin.left) || !is_zero(&bin.right) {
            return;
        }

        let desc = if op == BinaryOperator::GreaterEqualThan {
            "always true"
        } else {
            "always false"
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "This collection size check is {} \u{2014} `.length` and `.size` are never negative.",
                desc
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
