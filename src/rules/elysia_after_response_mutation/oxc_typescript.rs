//! elysia-after-response-mutation oxc backend — flag response mutation in onAfterResponse.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_assignment(text: &str, target: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = text[start..].find(target) {
        let after = start + pos + target.len();
        let rest = &text[after..];
        let next = rest.trim_start();
        if next.starts_with('=') && !next.starts_with("==") {
            return true;
        }
        start = after;
    }
    false
}

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
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "onAfterResponse" {
            return;
        }

        // Check arguments text by looking at the full call expression source.
        let call_text =
            &ctx.source[call.span.start as usize..call.span.end as usize];
        let has_header_mutation = call_text.contains("set.headers[")
            || call_text.contains("set.headers =");
        let has_status_mutation = has_assignment(call_text, "set.status");
        let has_return = call_text.contains("return ");
        if !has_header_mutation && !has_status_mutation && !has_return {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`onAfterResponse` cannot change the response — it runs after bytes are flushed. Move mutations to `onBeforeHandle` or `mapResponse`.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
