//! OXC backend for ts-no-dynamic-delete.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, UnaryOperator};
use oxc_span::GetSpan;
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
        let AstKind::UnaryExpression(unary) = node.kind() else { return };
        if unary.operator != UnaryOperator::Delete {
            return;
        }

        // Argument must be a computed member expression: obj[expr]
        let Expression::ComputedMemberExpression(member) = &unary.argument else {
            return;
        };

        // Allow literal string/number keys.
        match &member.expression {
            Expression::StringLiteral(_) | Expression::NumericLiteral(_) => return,
            // Allow negative number literals: `-42`
            Expression::UnaryExpression(inner)
                if inner.operator == UnaryOperator::UnaryNegation
                    && matches!(&inner.argument, Expression::NumericLiteral(_)) =>
            {
                return;
            }
            _ => {}
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.expression.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Do not delete dynamically computed property keys — use `Map` or `Set`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
