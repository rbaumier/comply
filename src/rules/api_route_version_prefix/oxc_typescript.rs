//! api-route-version-prefix oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options", "route",
];

const INFRA_PATHS: &[&str] = &[
    "/healthz", "/health", "/readyz", "/ready", "/livez", "/live", "/metrics",
];

/// Test, fixture, and mock infrastructure files are exempt: their route-shaped
/// calls (MSW handlers, fixtures) are deliberate test scaffolding, not server
/// routes. Delegates to the shared path classifier so the exemption stays in
/// sync with every other rule's test-directory handling.
fn is_test_file(path: &std::path::Path) -> bool {
    crate::rules::file_ctx::scan_path(path).in_test_dir
}

fn is_infra_path(path: &str) -> bool {
    INFRA_PATHS
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{p}/")))
        || path.starts_with("/dev/")
}

fn has_version_prefix(path: &str) -> bool {
    let p = path.strip_prefix("/api").unwrap_or(path);
    if !p.starts_with("/v") {
        return false;
    }
    let rest = &p[2..];
    let digit_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    if digit_end == 0 {
        return false;
    }
    digit_end == rest.len() || rest.as_bytes()[digit_end] == b'/'
}

fn extract_route_path<'a>(expr: &'a Expression<'a>, source: &'a str) -> Option<&'a str> {
    match expr {
        Expression::StringLiteral(lit) => {
            let s = lit.value.as_str();
            if s.starts_with('/') { Some(s) } else { None }
        }
        Expression::TemplateLiteral(tpl) => {
            if !tpl.expressions.is_empty() {
                return None;
            }
            let text = &source[tpl.span.start as usize + 1..tpl.span.end as usize - 1];
            if text.starts_with('/') { Some(text) } else { None }
        }
        _ => None,
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        if is_test_file(ctx.path) {
            return;
        }

        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let name = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&name) {
            return;
        }
        if receiver_is_http_client(member) {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(first_expr) = first_arg.as_expression() else {
            return;
        };
        let Some(route_path) = extract_route_path(first_expr, ctx.source) else {
            return;
        };

        // Distinguish a server route registration from a client HTTP call.
        // Verb methods (`get`, `post`, …) are overloaded: a framework router
        // registers `app.get("/users", handler)` (handler after the path) while
        // an HTTP client requests `client.get("/users")` (path only). The
        // router-specific `route` method has no client counterpart, so it stays
        // a route regardless of its arguments.
        if name != "route" && !call.arguments[1..].iter().any(is_handler_arg) {
            return;
        }
        if has_version_prefix(route_path) || is_infra_path(route_path) {
            return;
        }

        let span = first_expr.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Route `{route_path}` does not start with a version prefix (e.g. /v1/\u{2026})."
            ),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_route_without_version() {
        let d = run("app.get('/users', handler);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("/users"));
    }

    #[test]
    fn flags_post_without_version() {
        assert_eq!(run("router.post('/items', handler);").len(), 1);
    }

    #[test]
    fn flags_template_string_without_version() {
        assert_eq!(run("app.get(`/users`, handler);").len(), 1);
    }

    #[test]
    fn allows_versioned_route() {
        assert!(run("app.get('/v1/users', handler);").is_empty());
    }

    #[test]
    fn allows_v2() {
        assert!(run("app.put('/v2/items', handler);").is_empty());
    }

    #[test]
    fn allows_versioned_template_string() {
        assert!(run("app.delete(`/v1/users`, handler);").is_empty());
    }

    #[test]
    fn ignores_non_route_string() {
        assert!(run("app.get('someKey', handler);").is_empty());
    }

    #[test]
    fn ignores_dynamic_template() {
        assert!(run("app.get(`/users/${id}`, handler);").is_empty());
    }

    #[test]
    fn ignores_non_http_method() {
        assert!(run("map.set('/users', value);").is_empty());
    }

    #[test]
    fn ignores_plain_function_call() {
        assert!(run("get('/users');").is_empty());
    }

    #[test]
    fn ignores_test_file() {
        let d = crate::rules::test_helpers::run_rule(&Check, "app.get('/users', handler);", "src/routes.test.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_msw_mock_handlers_in_mocks_dir() {
        // Issue #1883: MSW mock handlers under `mocks/` intercept network
        // calls in test/example setup; their unversioned paths are deliberate.
        let src = "export const handlers = [\n  rest.get('/posts', (req, res, ctx) => res(ctx.json({}))),\n  rest.post('/posts', (req, res, ctx) => res(ctx.json({}))),\n];";
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            src,
            "examples/query/react/pagination/src/mocks/db.ts",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_jest_mocks_dir() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "app.get('/users', handler);",
            "src/__mocks__/server.ts",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn flags_production_route_outside_mocks() {
        // A real route file (not under mocks/test dirs) still flags.
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "app.get('/users', handler);",
            "src/api/routes.ts",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_route_method() {
        assert_eq!(run("app.route('/users').get(handler);").len(), 1);
    }

    #[test]
    fn rejects_v_without_digit() {
        assert_eq!(run("app.get('/vx/users', handler);").len(), 1);
    }

    #[test]
    fn allows_version_only_path() {
        assert!(run("app.get('/v1', handler);").is_empty());
    }

    #[test]
    fn allows_healthz() {
        assert!(run("app.get('/healthz', handler);").is_empty());
    }

    #[test]
    fn allows_health_check_variants() {
        assert!(run("app.get('/health', handler);").is_empty());
        assert!(run("app.get('/readyz', handler);").is_empty());
        assert!(run("app.get('/livez', handler);").is_empty());
        assert!(run("app.get('/metrics', handler);").is_empty());
    }

    #[test]
    fn allows_dev_endpoints() {
        assert!(run("app.get('/dev/last-reset-url', handler);").is_empty());
    }

    #[test]
    fn ignores_client_http_call_without_handler() {
        // Issue #1743 — `client.get("/path")` is a client-side HTTP request
        // (mande/axios/fetch wrapper), not a server route registration. With no
        // handler argument after the path it must not be flagged.
        assert!(run("jokes.get<Joke>('/jokes/random');").is_empty());
        assert!(run("api.post('/users', { name });").is_empty());
        assert!(run("axios.get('/jokes/random', config);").is_empty());
    }

    #[test]
    fn flags_route_with_arrow_handler() {
        // A genuine server route with an inline handler is still flagged.
        assert_eq!(run("app.get('/users', (req, res) => res.json([]));").len(), 1);
    }
}
