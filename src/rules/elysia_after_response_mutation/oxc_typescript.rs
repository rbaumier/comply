//! elysia-after-response-mutation oxc backend — flag response mutation in onAfterResponse.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        if !call_text.contains("set.headers")
            && !call_text.contains("set.status")
            && !call_text.contains("return ")
        {
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
