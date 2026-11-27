use std::sync::Arc;

use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind};
use oxc_semantic::{ReferenceFlags, Semantic};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, const_init, import_source_of, object_literal_property_value,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

/// msw 2 names the option `onUnhandledRequest`, msw 3 `onUnhandledFrame`.
const STRICTNESS_KEYS: &[&str] = &["onUnhandledRequest", "onUnhandledFrame"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        match member.property.name.as_str() {
            "listen" => {}
            // A dev worker may bypass by design. Tests only.
            "start" if is_test_file(ctx.path) => {}
            _ => return,
        }
        if !holds_msw_call(&member.object, semantic)
            || !is_lenient(call.arguments.first(), semantic)
        {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "MSW lets unhandled requests through — set `onUnhandledRequest: \"error\"` (msw 3: `onUnhandledFrame: \"error\"`).".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// `FileCtx::in_test_dir` also covers `mocks/`, home of the dev worker.
fn is_test_file(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.contains(".test.") || name.contains(".spec.")
}

/// An msw call, or a binding assigned one.
fn holds_msw_call(expr: &Expression, semantic: &Semantic) -> bool {
    let Expression::Identifier(id) = expr else {
        return is_msw_call(expr, semantic);
    };
    let scoping = semantic.scoping();
    let Some(symbol) = id
        .reference_id
        .get()
        .and_then(|r| scoping.get_reference(r).symbol_id())
    else {
        return false;
    };
    let nodes = semantic.nodes();
    let init_is_msw = matches!(nodes.kind(scoping.symbol_declaration(symbol)),
        AstKind::VariableDeclarator(decl) if decl.init.as_ref().is_some_and(|init| is_msw_call(init, semantic)));
    init_is_msw
        || scoping.get_resolved_references(symbol).any(|reference| {
            reference.flags().contains(ReferenceFlags::Write)
                && matches!(nodes.parent_kind(reference.node_id()),
                    AstKind::AssignmentExpression(assign) if is_msw_call(&assign.right, semantic))
        })
}

/// A call to `f()` or `ns.f()` imported from msw.
fn is_msw_call(expr: &Expression, semantic: &Semantic) -> bool {
    let Expression::CallExpression(call) = expr.without_parentheses() else {
        return false;
    };
    let root = match &call.callee {
        Expression::Identifier(id) => id,
        Expression::StaticMemberExpression(member) => match &member.object {
            Expression::Identifier(id) => id,
            _ => return false,
        },
        _ => return false,
    };
    import_source_of(root, semantic)
        .is_some_and(|source| source == "msw" || source.starts_with("msw/"))
}

/// True when the options leave unhandled requests unchecked.
fn is_lenient(options: Option<&Argument>, semantic: &Semantic) -> bool {
    let Some(options) = options else {
        return true;
    };
    let Some(Expression::ObjectExpression(object)) = options.as_expression() else {
        return false;
    };
    if object
        .properties
        .iter()
        .any(|p| matches!(p, ObjectPropertyKind::SpreadProperty(_)))
    {
        return false;
    }
    let strictness_values: Vec<_> = STRICTNESS_KEYS
        .iter()
        .filter_map(|key| object_literal_property_value(object, key))
        .collect();
    strictness_values.is_empty()
        || strictness_values
            .iter()
            .any(|value| is_lenient_value(value, semantic))
}

/// `"warn"`/`"bypass"`, written inline or held by a `const`.
fn is_lenient_value(value: &Expression, semantic: &Semantic) -> bool {
    let resolved = match value {
        Expression::Identifier(id) => const_init(id, semantic),
        other => Some(other),
    };
    matches!(resolved, Some(Expression::StringLiteral(lit)) if matches!(lit.value.as_str(), "warn" | "bypass"))
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

    fn run(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
    }

    #[test]
    fn flags_listen_without_options_in_test_file() {
        let src = r#"import { setupServer } from "msw/node";
const server = setupServer();
server.listen();
"#;
        let diagnostics = run(src, "src/api.test.ts");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].line, 3);
    }

    #[test]
    fn flags_listen_without_options_in_setup_files() {
        let src = r#"import { setupServer } from "msw/node";
const server = setupServer();
server.listen();
"#;
        for path in ["vitest.setup.ts", "src/mocks/server.ts"] {
            let diagnostics = run(src, path);
            assert_eq!(diagnostics.len(), 1, "{path}: {diagnostics:?}");
        }
    }

    #[test]
    fn flags_lenient_literal_and_const() {
        let bypass = r#"import { setupServer } from "msw/node";
setupServer().listen({ onUnhandledRequest: "bypass" });
"#;
        let diagnostics = run(bypass, "src/api.test.ts");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let held = r#"import { setupServer } from "msw/node";
const server = setupServer();
const MODE = "warn";
server.listen({ onUnhandledRequest: MODE });
"#;
        let diagnostics = run(held, "src/api.test.ts");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn flags_let_server_started_in_hook() {
        let src = r#"import { setupServer } from "msw/node";
let server;
beforeAll(() => {
  server = setupServer();
  server.listen();
});
"#;
        let diagnostics = run(src, "src/api.test.ts");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn allows_error_and_function_handler() {
        for options in [
            r#"{ onUnhandledRequest: "error" }"#,
            "{ onUnhandledRequest: (request) => {} }",
            "{ onUnhandledRequest: function (request) {} }",
        ] {
            let src = format!(
                "import {{ setupServer }} from \"msw/node\";\nsetupServer().listen({options});\n"
            );
            let diagnostics = run(&src, "src/api.test.ts");
            assert_eq!(diagnostics.len(), 0, "{options}: {diagnostics:?}");
        }
    }

    #[test]
    fn checks_msw3_on_unhandled_frame() {
        let strict = r#"import { setupServer } from "msw/node";
setupServer().listen({ onUnhandledFrame: "error" });
"#;
        let diagnostics = run(strict, "src/api.test.ts");
        assert_eq!(diagnostics.len(), 0, "{diagnostics:?}");
        let lenient = strict.replace("\"error\"", "\"warn\"");
        let diagnostics = run(&lenient, "src/api.test.ts");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn ignores_spread_options() {
        for options in ["{ ...opts }", r#"{ onUnhandledRequest: "warn", ...opts }"#] {
            let src = format!(
                "import {{ setupServer }} from \"msw/node\";\nsetupServer().listen({options});\n"
            );
            let diagnostics = run(&src, "src/api.test.ts");
            assert_eq!(diagnostics.len(), 0, "{options}: {diagnostics:?}");
        }
    }

    #[test]
    fn checks_worker_start_in_test_files_only() {
        let src = r#"import { setupWorker } from "msw/browser";
setupWorker().start({ onUnhandledRequest: "bypass" });
"#;
        let diagnostics = run(src, "src/mocks/browser.ts");
        assert_eq!(diagnostics.len(), 0, "{diagnostics:?}");
        let diagnostics = run(src, "src/app.test.ts");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn msw_fp_fixtures_stay_clean() {
        let deploy = r#"
            import { setupServer } from "msw/node";
            const mswServer = setupServer();
            mswServer.listen({ onUnhandledRequest: "error" });
        "#;
        let diagnostics = run(deploy, "src/server.test.ts");
        assert_eq!(diagnostics.len(), 0, "{diagnostics:?}");
        let schema = r#"
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
        let diagnostics = run(schema, "t.test.tsx");
        assert_eq!(diagnostics.len(), 0, "{diagnostics:?}");
        let version_prefix = "export const handlers = [\n  rest.get('/posts', (req, res, ctx) => res(ctx.json({}))),\n  rest.post('/posts', (req, res, ctx) => res(ctx.json({}))),\n];";
        let diagnostics = run(
            version_prefix,
            "examples/query/react/pagination/src/mocks/db.ts",
        );
        assert_eq!(diagnostics.len(), 0, "{diagnostics:?}");
    }

    #[test]
    fn resolves_msw_imports_not_names() {
        let local = "function setupServer() { return { listen() {} }; }\nsetupServer().listen();\n";
        let foreign = "import { setupServer } from \"my-mock-lib\";\nsetupServer().listen();\n";
        let aliased = "import { setupServer as mk } from \"msw/node\";\nmk().listen();\n";
        let namespace = "import * as msw from \"msw/node\";\nmsw.setupServer().listen();\n";
        for (src, expected) in [(local, 0), (foreign, 0), (aliased, 1), (namespace, 1)] {
            let diagnostics = run(src, "src/api.test.ts");
            assert_eq!(diagnostics.len(), expected, "{src}: {diagnostics:?}");
        }
    }
}
