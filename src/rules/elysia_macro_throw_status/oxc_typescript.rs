//! OXC backend for elysia-macro-throw-status.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(throw) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let text = &ctx.source[throw.span.start as usize..throw.span.end as usize];
        let norm: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        if !norm.contains("throwstatus(") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, throw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `return status(...)` instead of `throw status(...)` so Elysia tracks the response type.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
