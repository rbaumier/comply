//! elysia-numeric-body-no-coerce oxc backend — `t.Number()` inside a body schema
//! does not auto-coerce; flag and recommend `t.Numeric()`.

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

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop_text = member.property.name.as_str();
        const ROUTE_METHODS: &[&str] = &["post", "put", "patch", "delete"];
        if !ROUTE_METHODS.contains(&prop_text) {
            return;
        }

        let Some(args_span) = call.arguments_span() else { return };
        let args_text = &ctx.source[args_span.start as usize..args_span.end as usize];
        let norm: String = args_text.chars().filter(|c: &char| !c.is_whitespace()).collect();

        let Some(body_idx) = norm.find("body:t.") else { return };
        let after_body = &norm[body_idx..];

        let cut = ["params:", "query:", "headers:", "response:", "cookie:", "detail:", "tags:"]
            .iter()
            .filter_map(|k| after_body.find(k))
            .min()
            .unwrap_or(after_body.len());
        let body_section = &after_body[..cut];

        if !body_section.contains("t.Number(") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`t.Number()` in a `body:` schema rejects numeric strings — use `t.Numeric()` if the body can be form-encoded.".into(),
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
    fn flags_number_in_body() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ age: t.Number() }) });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_numeric_in_body() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ age: t.Numeric() }) });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_number_in_response_only() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ name: t.String() }), response: { 200: t.Object({ count: t.Number() }) } });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.post('/x', () => 'ok', { body: t.Object({ age: t.Number() }) });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
