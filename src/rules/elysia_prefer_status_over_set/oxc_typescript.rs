//! elysia-prefer-status-over-set oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::AssignmentExpression(assign) = node.kind() else { return };

        let left_span = assign.left.span();
        let left_text = &ctx.source[left_span.start as usize..left_span.end as usize];

        // Match `set.status` on the left side.
        if left_text != "set.status" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`set.status = code` is untyped \u{2014} use `status(code, body)` for type-safe responses.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
