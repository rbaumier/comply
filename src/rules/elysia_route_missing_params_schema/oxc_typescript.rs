//! elysia-route-missing-params-schema OXC backend — flag routes with `:param`
//! placeholders but no `params:` schema in options.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

/// Returns true if the identifier resolves to a binding imported from `"msw"` or `"msw/*"`.
fn ident_is_from_msw<'a>(
    ident: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
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

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop_text = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&prop_text) {
            return;
        }

        // Skip http.<method>(...) when the receiver binding is imported from msw.
        if let Expression::Identifier(obj) = &member.object
            && ident_is_from_msw(obj, semantic)
        {
            return;
        }

        // First argument should be a string literal path.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let arg_expr = first_arg.to_expression();
        let Expression::StringLiteral(path_lit) = arg_expr else {
            return;
        };
        let path = path_lit.value.as_str();

        // Check for `:param` segments.
        let has_param = path.split('/').any(|seg| seg.starts_with(':'));
        if !has_param {
            return;
        }

        // Check if `params:` appears in the full args text.
        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("params:") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route path declares `:param` but options have no `params:` schema — path params are unvalidated.".into(),
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
    fn ignores_msw_http_handler_with_param_path() {
        let src = r#"
import { http, HttpResponse } from "msw";
mswServer.use(
  http.get("*/api/v1/teams/:teamId", ({ params }) =>
    HttpResponse.json({ id: params.teamId, name: "X" })
  ),
);
"#;
        assert!(
            run_on(src).is_empty(),
            "MSW http.get with :param must not be flagged"
        );
    }

    #[test]
    fn flags_http_identifier_not_from_msw() {
        let src = r#"
import { http } from "./elysia-routes";
http.get("/users/:id", ({ params }) => params);
"#;
        assert_eq!(
            run_on(src).len(),
            1,
            "http.get with non-msw binding must still be flagged"
        );
    }

    /// Regression for issue #341: MSW handlers with `:param` paths inside Vitest .test.tsx
    /// component tests must not be flagged.
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
            crate::rules::test_helpers::run_oxc_tsx_with_framework(src, &Check, "elysia").is_empty(),
            "issue #341: MSW http.get with :param in .test.tsx must not be flagged by elysia-route-missing-params-schema"
        );
    }



    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/users/:id', () => 'ok');";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
