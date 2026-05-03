//! no-history-in-comments oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(raw) = ctx.source.get(start..end) else {
                continue;
            };
            if !super::mentions_history(raw) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Comment narrates history (`was`, `previously`, `refactored`, `rewritten`). Describe current behaviour — history lives in git log.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
