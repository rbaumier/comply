//! no-ignored-exceptions oxc backend — flag empty catch blocks.

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
        let AstKind::TryStatement(try_stmt) = node.kind() else {
            return;
        };

        let Some(handler) = &try_stmt.handler else {
            return;
        };

        // Check if the catch body has any real statements (not just empty).
        if !handler.body.body.is_empty() {
            return;
        }

        // OXC strips comments from the AST, so an empty body means either
        // truly empty or comment-only — both should be flagged (matching
        // the TreeSitter behaviour that also flags comment-only catch blocks).
        let (line, column) =
            byte_offset_to_line_col(ctx.source, handler.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Empty `catch` block silently swallows the exception \u{2014} log or re-throw it."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
