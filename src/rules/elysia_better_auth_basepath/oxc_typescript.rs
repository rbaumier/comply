//! elysia-better-auth-basepath OxcCheck backend.

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

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let callee_text =
            &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "betterAuth" {
            return;
        }

        // Get the full arguments text and check for invalid basePath patterns
        let args_start = call.span.start as usize + callee_text.len();
        let args_end = call.span.end as usize;
        let args_text = &ctx.source[args_start..args_end.min(ctx.source.len())];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        let invalid = norm.contains("basePath:''")
            || norm.contains("basePath:\"\"")
            || norm.contains("basePath:'/'")
            || norm.contains("basePath:\"/\"");
        if !invalid {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message:
                "`betterAuth` `basePath` cannot be empty or `'/'` — set a real prefix like `'/api/auth'`."
                    .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
