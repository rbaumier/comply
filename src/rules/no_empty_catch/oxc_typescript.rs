//! no-empty-catch oxc backend — flag `catch (e) {}` with an empty body.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else { return };
        let Some(handler) = &try_stmt.handler else { return };
        let body = &handler.body;

        if !body.body.is_empty() {
            return;
        }

        // Allow catch blocks that contain comments.
        let body_text = &ctx.source[body.span.start as usize..body.span.end as usize];
        if body_text.contains("//") || body_text.contains("/*") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, handler.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Empty catch block silently swallows the error — log it, rethrow, \
                      or add a comment explaining why."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
