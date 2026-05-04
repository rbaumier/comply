use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_not_promise(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::ArrayExpression(_)
            | Expression::ArrowFunctionExpression(_)
            | Expression::BooleanLiteral(_)
            | Expression::ClassExpression(_)
            | Expression::FunctionExpression(_)
            | Expression::JSXElement(_)
            | Expression::JSXFragment(_)
            | Expression::NullLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::RegExpLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::TemplateLiteral(_)
            | Expression::UnaryExpression(_)
            | Expression::UpdateExpression(_)
            | Expression::BinaryExpression(_)
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else { return };

        // Unwrap parenthesized expression layers.
        let mut unwrapped = &await_expr.argument;
        while let Expression::ParenthesizedExpression(paren) = unwrapped {
            unwrapped = &paren.expression;
        }

        // For sequence expressions, check the last expression.
        let check_expr = if let Expression::SequenceExpression(seq) = unwrapped {
            match seq.expressions.last() {
                Some(last) => last,
                None => return,
            }
        } else {
            unwrapped
        };

        if !is_not_promise(check_expr) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Do not `await` a non-promise value.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
