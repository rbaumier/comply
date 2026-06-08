//! OxcCheck backend — flag heavy work inside `.onRequest()`.

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
        // Callee must be `*.onRequest`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "onRequest" {
            return;
        }
        // Check the arguments text for heavy patterns.
        let args_start = call.span.start as usize;
        let args_end = call.span.end as usize;
        let args_text = &ctx.source[args_start..args_end];

        let heavy = args_text.contains("await ")
            || args_text.contains("fetch(")
            || args_text.contains("db.")
            || args_text.contains("prisma.")
            || args_text.contains("JSON.parse");
        if !heavy {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.onRequest()` runs before routing on every request \u{2014} move heavy work (await/fetch/db/JSON.parse) to `.beforeHandle()`.".into(),
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
    fn flags_await_in_on_request() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onRequest(async ({ request }) => { await fetch('/x'); });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_db_in_on_request() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onRequest(({ request }) => { db.query('SELECT 1'); });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_lightweight_on_request() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onRequest(({ request }) => { console.log(request.url); });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.onRequest(async () => { await fetch('/x'); });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
