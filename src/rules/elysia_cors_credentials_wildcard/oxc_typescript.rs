//! elysia-cors-credentials-wildcard oxc backend — flag credentials:true without specific origin.

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
        if callee_text != "cors" {
            return;
        }

        let args_start = call.span.start as usize;
        let args_end = call.span.end as usize;
        let call_text = &ctx.source[args_start..args_end];
        let norm: String = call_text.chars().filter(|c| !c.is_whitespace()).collect();

        if !norm.contains("credentials:true") {
            return;
        }

        let has_specific_origin = norm.contains("origin:")
            && !norm.contains("origin:'*'")
            && !norm.contains("origin:\"*\"")
            && !norm.contains("origin:true");

        if !has_specific_origin {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`credentials: true` without a specific origin — browsers reject wildcard origins with credentials.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_credentials_without_origin() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ credentials: true }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_credentials_with_specific_origin() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: 'https://x.com', credentials: true }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.use(cors({ credentials: true }));";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
