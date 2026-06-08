//! OxcCheck backend — flag `.ws(` reading headers without `headers:` schema.

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
        if !callee_text.ends_with(".ws") {
            return;
        }

        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];

        let reads_headers = args_text.contains("headers.authorization")
            || args_text.contains("headers['authorization']")
            || args_text.contains("headers[\"authorization\"]")
            || args_text.contains("headers.cookie");
        if !reads_headers {
            return;
        }

        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("headers:t.") || norm.contains("header:t.") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "WebSocket route reads request headers but declares no `headers:` schema — header presence is not enforced.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_headers_read_without_schema() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().ws('/chat', { beforeHandle({ headers }) { const t = headers.authorization; } });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_headers_with_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().ws('/chat', { headers: t.Object({ authorization: t.String() }), beforeHandle({ headers }) { const x = headers.authorization; } });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src =
            "app.ws('/chat', { beforeHandle({ headers }) { const t = headers.authorization; } });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
