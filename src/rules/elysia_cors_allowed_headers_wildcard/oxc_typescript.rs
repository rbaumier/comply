//! elysia-cors-allowed-headers-wildcard oxc backend — flag wildcard allowedHeaders with credentials.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let callee_text =
            &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "cors" {
            return;
        }

        let args_start = call.span.start as usize + callee_text.len();
        let args_text = &ctx.source[args_start..call.span.end as usize];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        if !norm.contains("credentials:true") {
            return;
        }

        let wildcard = norm.contains("allowedHeaders:'*'") || norm.contains("allowedHeaders:\"*\"");
        let omitted = !norm.contains("allowedHeaders:");
        if !wildcard && !omitted {
            return;
        }

        let msg = if wildcard {
            "`cors({ credentials: true, allowedHeaders: '*' })` is rejected by browsers — list explicit headers."
        } else {
            "`cors({ credentials: true })` without `allowedHeaders` falls back to the wildcard, which browsers reject — list explicit headers."
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: msg.into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
