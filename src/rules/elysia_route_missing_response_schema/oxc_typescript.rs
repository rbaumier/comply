//! OxcCheck backend — flag Elysia routes that validate input but lack `response:`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

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
        // Callee must be `*.get` / `*.post` / etc.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&prop) {
            return;
        }
        // Get the full call text and normalize whitespace for keyword matching.
        let call_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = call_text.chars().filter(|c| !c.is_whitespace()).collect();

        let validates_input = norm.contains("body:") || norm.contains("params:");
        if !validates_input {
            return;
        }
        if norm.contains("response:") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route validates input but has no `response:` schema \u{2014} Eden/OpenAPI clients lose the success type.".into(),
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
    fn flags_post_with_body_no_response() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ a: t.String() }) });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_post_with_response_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ a: t.String() }), response: { 200: t.Object({ ok: t.Boolean() }) } });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_route_without_validation() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/x', () => 'ok');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.post('/x', () => 'ok', { body: 1 });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
