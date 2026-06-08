//! elysia-derive-validated-data oxc backend — flag `.derive(` callbacks reading body/params/query.

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
        if !callee_text.ends_with(".derive") {
            return;
        }

        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];

        let touches_validated = args_text.contains("body")
            || args_text.contains("params")
            || args_text.contains("query");
        if !touches_validated {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.derive()` reads `body`/`params`/`query` before validation — use `.resolve()` to access validated data.".into(),
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
    fn flags_derive_reading_body() {
        let src =
            "import { Elysia } from 'elysia';\nnew Elysia().derive(({ body }) => ({ b: body }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_derive_reading_only_headers() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().derive(({ headers }) => ({ token: headers.authorization }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "obj.derive(({ body }) => body);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
