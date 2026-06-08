//! elysia-mapresponse-sync-compression oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["deflateSync", "gzipSync"])
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

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "mapResponse" {
            return;
        }

        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        if !args_text.contains("gzipSync") && !args_text.contains("deflateSync") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`mapResponse` is on the hot path \u{2014} synchronous `gzipSync` / `deflateSync` blocks the event loop. Use the async `zlib/promises` variants.".into(),
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
    fn flags_gzip_sync() {
        let src = "import { Elysia } from 'elysia';\napp.mapResponse(({ response }) => gzipSync(response));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_deflate_sync() {
        let src = "import { Elysia } from 'elysia';\napp.mapResponse(({ response }) => deflateSync(Buffer.from(response)));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_async_gzip() {
        let src = "import { Elysia } from 'elysia';\nimport { gzip } from 'zlib/promises';\napp.mapResponse(async ({ response }) => await gzip(response));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.mapResponse(({ response }) => gzipSync(response));";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
