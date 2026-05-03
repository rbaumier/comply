//! OXC backend for prefer-math-min-max.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BinaryOperator;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ConditionalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ConditionalExpression(cond) = node.kind() else {
            return;
        };

        // The test must be a binary comparison.
        let oxc_ast::ast::Expression::BinaryExpression(test) = &cond.test else {
            return;
        };

        let op = test.operator;
        let is_gt = matches!(op, BinaryOperator::GreaterThan | BinaryOperator::GreaterEqualThan);
        let is_lt = matches!(op, BinaryOperator::LessThan | BinaryOperator::LessEqualThan);
        if !is_gt && !is_lt {
            return;
        }

        let left_text = &ctx.source[test.left.span().start as usize..test.left.span().end as usize];
        let right_text = &ctx.source[test.right.span().start as usize..test.right.span().end as usize];
        let cons_text = &ctx.source[cond.consequent.span().start as usize..cond.consequent.span().end as usize];
        let alt_text = &ctx.source[cond.alternate.span().start as usize..cond.alternate.span().end as usize];

        let left_text = left_text.trim();
        let right_text = right_text.trim();
        let cons_text = cons_text.trim();
        let alt_text = alt_text.trim();

        if left_text.is_empty() || right_text.is_empty() {
            return;
        }

        let method: Option<&str> = if (is_gt && left_text == alt_text && right_text == cons_text)
            || (is_lt && left_text == cons_text && right_text == alt_text)
        {
            Some("min")
        } else if (is_gt && left_text == cons_text && right_text == alt_text)
            || (is_lt && left_text == alt_text && right_text == cons_text)
        {
            Some("max")
        } else {
            None
        };

        if let Some(method) = method {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, cond.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Prefer `Math.{method}({left_text}, {right_text})` over this ternary."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
