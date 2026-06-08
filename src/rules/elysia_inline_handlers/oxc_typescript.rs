//! OxcCheck backend for elysia-inline-handlers.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options",
];

/// Returns true if the identifier resolves to a binding imported from `"msw"` or `"msw/*"`.
fn ident_is_from_msw<'a>(
    ident: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id)) {
        if let AstKind::ImportDeclaration(import) = kind {
            let src = import.source.value.as_str();
            return src == "msw" || src.starts_with("msw/");
        }
    }
    false
}

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
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&prop) {
            return;
        }

        // Skip `Reflect.get(...)` / `Reflect.apply(...)` — the JS reflection
        // built-in, whose method names collide with Elysia route methods but
        // have nothing to do with route registration (e.g. inside a Proxy trap).
        if let Expression::Identifier(obj) = &member.object
            && obj.name.as_str() == "Reflect"
        {
            return;
        }

        // Skip http.<method>(...) when the receiver binding is imported from msw.
        if let Expression::Identifier(obj) = &member.object
            && ident_is_from_msw(obj, semantic)
        {
            return;
        }

        // Need at least 2 args: path + handler.
        if call.arguments.len() < 2 {
            return;
        }

        let Some(handler_expr) = call.arguments[1].as_expression() else { return };
        match handler_expr {
            // Inline handlers are fine.
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => return,
            // Literals are fine (static responses).
            Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::ObjectExpression(_)
            | Expression::ArrayExpression(_)
            | Expression::TemplateLiteral(_) => return,
            // Identifier or member expression = handler by reference.
            Expression::Identifier(_) | Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_) => {}
            _ => return,
        }

        let handler_span = handler_expr.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, handler_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route handler passed by reference loses Elysia's type inference. Wrap in an inline arrow function.".into(),
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn ignores_msw_http_handlers() {
        let src = r#"
import { http, HttpResponse } from "msw";
import { mswServer } from "./test-helpers/setup-msw";
mswServer.use(
  http.get("*/api/v1/teams", () => HttpResponse.json({ items: [] })),
);
"#;
        assert!(
            run_on(src).is_empty(),
            "MSW http.get handlers must not be flagged"
        );
    }

    #[test]
    fn ignores_msw_http_handler_by_reference() {
        let src = r#"
import { http } from "msw";
import { listTeamsHandler } from "./handlers";
mswServer.use(http.post("*/api/v1/teams", listTeamsHandler));
"#;
        assert!(
            run_on(src).is_empty(),
            "MSW http.post by reference must not be flagged"
        );
    }

    #[test]
    fn ignores_msw_http_alias() {
        // `http` imported as `mockHttp` from msw — alias must still be exempted via binding.
        let src = r#"
import { http as mockHttp } from "msw";
import { listTeamsHandler } from "./handlers";
mswServer.use(mockHttp.get("*/api/v1/teams", listTeamsHandler));
"#;
        assert!(
            run_on(src).is_empty(),
            "aliased MSW http must not be flagged"
        );
    }

    #[test]
    fn flags_http_identifier_not_from_msw() {
        // `http` is imported from a local module, not msw — binding check must still flag it.
        let src = r#"
import { http } from "./elysia-routes";
http.get("/", handleFn);
"#;
        assert_eq!(
            run_on(src).len(),
            1,
            "http.get with non-msw binding must still be flagged"
        );
    }

    // Regression for issues #536 / #652: `Reflect.get` / `Reflect.apply` inside a
    // ProxyHandler trap are the JS reflection built-in, not Elysia route handlers.
    #[test]
    fn ignores_reflect_get_in_proxy_trap() {
        let src = r#"
            const handler: ProxyHandler<postgres.Sql> = {
                get(_target, prop) {
                    const source = als.getStore() ?? rawPg;
                    const propertyValue = Reflect.get(source, prop);
                    return propertyValue;
                },
            };
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn ignores_reflect_apply_in_proxy_trap() {
        let src = r#"
            const handler: ProxyHandler<postgres.Sql> = {
                apply(_target, _thisArg, args) {
                    return Reflect.apply(_target, _thisArg, args);
                },
            };
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    /// Regression for issue #341: MSW handlers inside Vitest component tests (.test.tsx) must
    /// not be flagged even when the URL contains `:param` placeholders.
    #[test]
    fn ignores_msw_handler_in_tsx_test_file() {
        let src = r#"
import { http, HttpResponse } from "msw";
import { mswServer } from "@/app/test-helpers/msw-server";

it("fetches data", () => {
  mswServer.use(
    http.get("*/api/v1/organizations/:organizationId", () =>
      HttpResponse.json({ id: "1", name: "Foo" }),
    ),
  );
});
"#;
        assert!(
            crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx()).is_empty(),
            "issue #341: MSW http.get in .test.tsx must not be flagged by elysia-inline-handlers"
        );
    }
}
