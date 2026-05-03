//! OxcCheck backend — flag unconditional `fromTypes('src/...')` calls.

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
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "fromTypes" {
            return;
        }

        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];

        let has_src_path = args_text.contains("'src/") || args_text.contains("\"src/");
        if !has_src_path {
            return;
        }
        if args_text.contains("process.env") || args_text.contains("NODE_ENV") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`fromTypes('src/...')` reads source at runtime — gate it behind a NODE_ENV check or pre-build the spec.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
