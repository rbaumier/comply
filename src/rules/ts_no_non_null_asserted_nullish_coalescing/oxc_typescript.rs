//! ts-no-non-null-asserted-nullish-coalescing OXC backend — flag
//! `x! ?? y` where TSNonNullExpression is the left operand of a `??`
//! logical expression.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalOperator};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LogicalExpression(logical) = node.kind() else {
            return;
        };

        if logical.operator != LogicalOperator::Coalesce {
            return;
        }

        let Expression::TSNonNullExpression(non_null) = &logical.left else {
            return;
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, non_null.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`x! ?? y` is contradictory — the `!` asserts non-null \
                      while `??` handles null. Remove the `!`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
