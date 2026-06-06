//! hono-no-unvalidated-body OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

fn is_hono_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "hono") || crate::oxc_helpers::source_contains(source, "Hono")
}

fn has_validator(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "hono/validator")
        || crate::oxc_helpers::source_contains(source, "@hono/zod-validator")
        || crate::oxc_helpers::source_contains(source, "@hono/typebox-validator")
        || crate::oxc_helpers::source_contains(source, "@hono/valibot-validator")
        || crate::oxc_helpers::source_contains(source, "zValidator")
        || crate::oxc_helpers::source_contains(source, "tbValidator")
        || crate::oxc_helpers::source_contains(source, "vValidator")
        || crate::oxc_helpers::source_contains(source, "validator(")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono", "Hono"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !is_hono_file(ctx.source) { return; }
        if has_validator(ctx.source) { return; }

        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];

        let is_json = callee_text.ends_with(".req.json");
        let is_parse_body = callee_text.ends_with(".req.parseBody");
        if !is_json && !is_parse_body { return; }

        let method = if is_json { "json" } else { "parseBody" };
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`c.req.{method}()` reads the request body without schema validation — add a validator middleware and use `c.req.valid(...)`."),
            severity: Severity::Warning,
            span: None,
        });
    }
}
