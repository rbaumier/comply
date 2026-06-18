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
        semantic: &'a oxc_semantic::Semantic<'a>,
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
        // Skip `http.<method>(...)` when the receiver binding is imported from msw —
        // MSW request handlers share Elysia's route call shape.
        if crate::rules::elysia_helpers::member_receiver_is_from_msw(member, semantic) {
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            "t.ts",
            &crate::project::ProjectCtx::for_test_with_framework("elysia"),
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn flags_route_validating_input_without_response_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ a: t.String() }) });";
        assert_eq!(
            run_on(src).len(),
            1,
            "real Elysia route with body schema but no response schema must still flag"
        );
    }

    #[test]
    fn allows_route_with_response_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ a: t.String() }), response: { 200: t.Object({ ok: t.Boolean() }) } });";
        assert!(run_on(src).is_empty());
    }

    /// Regression for issue #4056: MSW request handlers share Elysia's
    /// `<obj>.<method>(path, handler)` shape — `http.put(...)` returning JSON
    /// with no `response:` schema must not be flagged. The resolver body here
    /// contains `body:` (in `calls.push({ ..., body: ... })`), which would
    /// otherwise satisfy the rule's input-validation trigger.
    #[test]
    fn ignores_msw_http_handler_in_test_file() {
        let src = r#"
import { http, HttpResponse } from "msw";
import { mswServer } from "@/app/test-helpers/msw-server";

const calls: unknown[] = [];

mswServer.use(
  http.put(
    "*/api/v1/products/:productId/central-correspondences/:centraleId",
    async ({ request, params }) => {
      calls.push({ centraleId: String(params["centraleId"]), body: await request.json() });
      return HttpResponse.json({ ok: true });
    },
  ),
  http.get("*/api/v1/products/:productId", async ({ params }) => {
    return HttpResponse.json({ id: params["productId"], body: "x" });
  }),
  http.post("*/api/v1/products", async ({ request }) => {
    return HttpResponse.json({ created: true, body: await request.json() });
  }),
);
"#;
        assert!(
            crate::rules::test_helpers::run_rule_with_ctx(
                &Check,
                src,
                "t.test.tsx",
                &crate::project::ProjectCtx::for_test_with_framework("elysia"),
                crate::rules::file_ctx::default_static_file_ctx(),
            )
            .is_empty(),
            "issue #4056: MSW http.* handlers must not be flagged by elysia-route-missing-response-schema"
        );
    }

    #[test]
    fn flags_http_identifier_not_from_msw() {
        let src = r#"
import { http } from "./elysia-routes";
http.post("/x", ({ body }) => body, { body: 1 });
"#;
        assert_eq!(
            run_on(src).len(),
            1,
            "http.post with non-msw binding must still flag"
        );
    }
}
