//! api-first oxc backend — flag files that register an HTTP route without
//! referencing any schema validator.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "delete"];
const SCHEMA_INDICATORS: &[&str] = &["z", "createRoute", "openapi", "schema", "zodValidator"];

pub struct Check;

/// True when `arg` is a string literal whose value begins with `/`.
/// Route registrations always take a path-string as the first argument;
/// `Headers#get("name")`, `Map#get(key)`, `URLSearchParams#get("q")` do not.
fn first_arg_is_route_path(arg: &Argument) -> bool {
    match arg {
        Argument::StringLiteral(s) => s.value.starts_with('/'),
        Argument::TemplateLiteral(t) => t
            .quasis
            .first()
            .is_some_and(|q| q.value.raw.starts_with('/')),
        _ => false,
    }
}

/// Receiver names that denote an HTTP client instance rather than a framework
/// router/app. `axios.get(url, config)` and `client.get(url)` request a URL;
/// they do not register a server route.
const HTTP_CLIENT_RECEIVERS: &[&str] = &[
    "axios", "http", "https", "client", "fetch", "request", "req", "instance",
];

/// True when `arg` is a request handler: a function, or a reference to one
/// (`handler`, `controller.list`). A server route registration passes a handler
/// after the path (`app.get("/users", handler)`); a client HTTP call
/// (`client.get("/users")`) passes only the path.
fn is_handler_arg(arg: &Argument) -> bool {
    matches!(
        arg,
        Argument::ArrowFunctionExpression(_)
            | Argument::FunctionExpression(_)
            | Argument::Identifier(_)
            | Argument::StaticMemberExpression(_)
            | Argument::ComputedMemberExpression(_)
    )
}

/// True when the call's receiver is a known HTTP-client name.
fn receiver_is_http_client(member: &oxc_ast::ast::StaticMemberExpression) -> bool {
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    HTTP_CLIENT_RECEIVERS.contains(&obj.name.as_str())
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Quick check: if any schema indicator appears in source, skip.
        if SCHEMA_INDICATORS.iter().any(|s| ctx.source_contains(s)) {
            return Vec::new();
        }

        // Find the first route call: `<recv>.<method>("/...", handler)` with
        // method in ROUTE_METHODS. Requiring a path-literal first argument
        // excludes `Headers#get("name")`, `Map#get(key)`, etc.
        let mut route_span = None;
        for snode in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = snode.kind() else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            let method = member.property.name.as_str();
            if !ROUTE_METHODS.contains(&method) {
                continue;
            }
            if receiver_is_http_client(member) {
                continue;
            }
            let Some(first_arg) = call.arguments.first() else {
                continue;
            };
            if !first_arg_is_route_path(first_arg) {
                continue;
            }
            // A server route registration passes a handler after the path; a
            // client HTTP call (`client.get("/users")`, `axios.get(url, config)`)
            // does not. Requiring a handler excludes client wrappers.
            if !call.arguments[1..].iter().any(is_handler_arg) {
                continue;
            }
            let start = call.span.start;
            if route_span.is_none_or(|s: u32| start < s) {
                route_span = Some(start);
            }
        }

        let Some(span_start) = route_span else {
            return Vec::new();
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route handler without schema definition — define the API schema (e.g. `z.object`, `zodValidator`) before the handler.".into(),
            severity: Severity::Warning,
            span: None,
        }]
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
    
    fn run_on(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_route_without_schema() {
        let src = r#"app.get("/users", (c) => { return c.json([]); });"#;
        assert_eq!(run_on(src, "src/api/users.ts").len(), 1);
    }

    #[test]
    fn allows_route_with_zod_schema() {
        let src = r#"
const querySchema = z.object({ page: z.number() });
app.get("/users", zodValidator("query", querySchema), (c) => { return c.json([]); });
"#;
        assert!(run_on(src, "src/api/users.ts").is_empty());
    }

    #[test]
    fn allows_non_route_file() {
        let src = r#"export function getUsers() { return db.query("SELECT * FROM users"); }"#;
        assert!(run_on(src, "src/lib/users.ts").is_empty());
    }

    #[test]
    fn ignores_headers_get_non_route() {
        // Regression for #87 — `Response#headers.get("name")` is the
        // Web Platform `Headers.get()` method, not a route registration: the
        // argument does not start with `/`, so it is never a route.
        let src = r#"
const res = await app.handle(new Request("http://example.test/"));
const exposeHeaders = res.headers.get("access-control-expose-headers");
"#;
        assert!(run_on(src, "src/api/middleware/composition.ts").is_empty());
    }

    #[test]
    fn skips_route_in_integration_helpers_issue3389() {
        // Issue #3389 — ephemeral Express servers under a top-level
        // `integration/helpers/` directory are Playwright test infra, never
        // deployed. The engine gate (`skip_in_test_dir` + `FileCtx::in_test_dir`)
        // exempts them; the raw check still fires on the same code in src/.
        let src = r#"const app = express(); app.get("/users", (req, res) => res.json([]));"#;
        assert_eq!(run_on(src, "src/api/users.ts").len(), 1);
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "integration/helpers/rsc-vite/server.js"
            )
            .is_empty()
        );
    }

    #[test]
    fn skips_route_in_flat_test_dir_issue3302() {
        // Issue #3302 — ky uses a flat `test/` directory whose files are named
        // `test/bytes.ts`, `tests/context.ts` etc., without a `.test.`/`.spec.`
        // suffix. They register ephemeral test servers, not deployed routes.
        // The central `skip_in_test_dir` gate (`FileCtx::in_test_dir` covers a
        // top-level `test/`/`tests/` directory) exempts them; the same handler
        // in a production route file still fires.
        let src = r#"server.get('/', (request, response) => { response.end(Buffer.from([0, 1, 2, 255])); });"#;
        assert!(crate::rules::test_helpers::run_rule_gated(&Check, src, "test/bytes.ts").is_empty());
        assert!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "tests/context.ts").is_empty()
        );
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "src/server/routes.ts").len(),
            1
        );
    }

    #[test]
    fn ignores_headers_get_outside_test_file() {
        // Even outside test files, `.get("name")` with a non-`/`
        // string argument is not a route registration.
        let src = r#"
function readHeader(res: Response): string | null {
    return res.headers.get("x-request-id");
}
"#;
        assert!(run_on(src, "src/api/util/headers.ts").is_empty());
    }

    #[test]
    fn ignores_client_http_call_without_handler() {
        // Issue #1743 — `jokes.get("/jokes/random")` is a client-side HTTP call
        // (mande wraps fetch), not a server route handler. With no handler
        // argument after the path it must not be flagged as a route.
        let src = r#"
import { createClient } from 'mande'
const jokes = createClient('https://v2.jokeapi.dev')
export function getRandomJoke() {
  return jokes.get<Joke>('/jokes/random')
}
"#;
        assert!(run_on(src, "packages/playground/src/api/jokes.ts").is_empty());
    }

    #[test]
    fn ignores_axios_call_with_config_object() {
        // `axios.get(url, config)` — the trailing argument is an options object,
        // not a handler, so it is a client call, not a route registration.
        let src = r#"export function load() { return axios.get('/users', { params }); }"#;
        assert!(run_on(src, "src/api/users.ts").is_empty());
    }

    #[test]
    fn ignores_map_get_with_non_path_arg() {
        // `Map#get(key)` / `URLSearchParams#get(name)` — neither argument
        // starts with `/`, so neither looks like a route path.
        let src = r#"
const params = new URLSearchParams(req.url);
const q = params.get("q");
const cached = cache.get(key);
"#;
        assert!(run_on(src, "src/api/search.ts").is_empty());
    }
}
