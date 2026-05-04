use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new URL"])
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

        let body_text =
            &ctx.source[try_stmt.block.span.start as usize..try_stmt.block.span.end as usize];

        if !body_text.contains("new URL(") {
            return;
        }

        let Some(handler) = &try_stmt.handler else {
            return;
        };

        let catch_text =
            &ctx.source[handler.body.span.start as usize..handler.body.span.end as usize];

        let is_validation_pattern = body_text.contains("return true")
            || body_text.contains("return new URL")
            || catch_text.contains("return false")
            || catch_text.contains("return null")
            || catch_text.contains("return undefined");

        if !is_validation_pattern {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, try_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `URL.canParse(url)` instead of try-catch with `new URL()`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
