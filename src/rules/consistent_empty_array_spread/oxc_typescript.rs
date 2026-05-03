//! OXC backend for consistent-empty-array-spread.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SpreadElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SpreadElement(spread) = node.kind() else { return };

        // If the spread argument is a conditional (ternary), it's unparenthesized.
        // A parenthesized ternary would be wrapped in ParenthesizedExpression.
        if !matches!(spread.argument, Expression::ConditionalExpression(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, spread.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Parenthesize the ternary in array spread: \
                      `[...(condition ? ['a'] : [])]`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
