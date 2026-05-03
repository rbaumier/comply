//! ts-no-confusing-non-null-assertion oxc backend — flag `a! == b` which
//! looks confusingly like `a !== b`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

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

        let is_confusing = matches!(
            bin.operator,
            BinaryOperator::Equality
                | BinaryOperator::StrictEquality
                | BinaryOperator::Instanceof
                | BinaryOperator::In
        );
        if !is_confusing {
            return;
        }

        if !matches!(&bin.left, Expression::TSNonNullExpression(_)) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, bin.left.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Confusing non-null assertion before comparison — \
                      `a! == b` looks like `a !== b`. Remove the `!` or \
                      wrap in parentheses."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
