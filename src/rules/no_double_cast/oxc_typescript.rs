//! no-double-cast OXC backend — flag `x as unknown as T` style double casts.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAsExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAsExpression(as_expr) = node.kind() else { return };

        // Double casts are the standard pattern for test doubles / partial stubs.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // The inner expression of `x as A as B` is itself a TSAsExpression.
        if !matches!(&as_expr.expression, Expression::TSAsExpression(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Double cast `as X as Y` hides misaligned types. \
                      Fix the real problem: align the interface, or \
                      validate at the boundary with a type guard or Zod \
                      schema that actually checks the shape at runtime."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
