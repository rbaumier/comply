//! de-morgan-simplify OXC backend — flag `!(a && b)` / `!(a || b)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalOperator, UnaryOperator};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UnaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UnaryExpression(unary) = node.kind() else {
            return;
        };
        if unary.operator != UnaryOperator::LogicalNot {
            return;
        }

        // Argument must be parenthesized expression containing a logical expression.
        let Expression::ParenthesizedExpression(paren) = &unary.argument else {
            return;
        };
        let Expression::LogicalExpression(logical) = &paren.expression else {
            return;
        };

        let (op_str, suggested) = match logical.operator {
            LogicalOperator::And => ("&&", "||"),
            LogicalOperator::Or => ("||", "&&"),
            _ => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, unary.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Apply De Morgan's law: `!(a {op_str} b)` simplifies to `!a {suggested} !b`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
