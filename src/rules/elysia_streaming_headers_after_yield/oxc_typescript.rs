//! elysia-streaming-headers-after-yield OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
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

        let span = match node.kind() {
            AstKind::Function(f) => f.span,
            AstKind::ArrowFunctionExpression(f) => f.span,
            _ => return,
        };

        let start = span.start as usize;
        let end = span.end as usize;
        let body_text = &ctx.source[start..end.min(ctx.source.len())];

        let Some(yield_idx) = body_text.find("yield") else {
            return;
        };
        let Some(headers_idx) = body_text.find("set.headers") else {
            return;
        };
        if headers_idx <= yield_idx {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`set.headers` is assigned after a `yield` — headers are already flushed once the stream starts. Move header writes before the first yield."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
