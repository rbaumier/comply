//! api-route-version-prefix oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options", "route",
];

/// Standardized operational/infrastructure endpoints that are unversioned by
/// convention: Kubernetes liveness/readiness probes (`/healthz`, `/readyz`,
/// `/livez`, and the bare/underscore variants orchestrators and load balancers
/// poll), the Prometheus scrape target (`/metrics`), and the conventional
/// `/ping`, `/status`, `/version` checks plus crawler files (`/favicon.ico`,
/// `/robots.txt`). These are infrastructure-level, not API resources; a version
/// prefix like `/v1/healthz` would break every probe and orchestration config.
const INFRA_PATHS: &[&str] = &[
    "/healthz", "/health", "/_health", "/_healthz",
    "/readyz", "/ready", "/_ready", "/_readyz",
    "/livez", "/live", "/_live", "/_livez",
    "/metrics", "/ping", "/status", "/version",
    "/favicon.ico", "/robots.txt",
];

/// OAuth 2.0 (RFC 6749) and OpenID Connect Core 1.0 protocol endpoints. Clients,
/// SDKs, and registered redirect URIs expect these exact paths, so a version
/// prefix like `/v1/authorize` would break the protocol; they must not be flagged.
const OAUTH_OIDC_PATHS: &[&str] = &[
    "/authorize",
    "/callback",
    "/token",
    "/userinfo",
    "/me",
    "/introspect",
    "/revoke",
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

fn is_oauth_oidc_path(path: &str) -> bool {
    OAUTH_OIDC_PATHS.contains(&path)
}

/// IANA-reserved "well-known" URI namespace (RFC 5785). Sub-paths such as
/// `/.well-known/openid-configuration`, `/.well-known/security.txt`, and
/// `/.well-known/appspecific/...` are registered at fixed standardized paths;
/// a version prefix like `/v1/.well-known/...` would break discovery.
fn is_well_known_path(path: &str) -> bool {
    path.starts_with("/.well-known/")
}

/// GraphQL-paradigm routes are a single, unversioned URL by convention: the
/// GraphQL spec and every major server/client (Apollo, Yoga, Pothos, Mercurius)
/// default to `/graphql`, and versioning is done in the schema (field
/// deprecation, additive changes), not in the URL. The same holds for the GraphQL
/// in-browser IDEs served alongside it — GraphiQL (`/graphiql`) and the
/// `/playground` UI — which are static developer tools, not versioned resources.
/// True when any `/`-delimited segment of the path is exactly `graphql`,
/// `graphiql`, or `playground` — covering the canonical endpoints, mounts like
/// `/api/graphql`, sub-paths like `/graphql/stream`, and the IDE asset paths
/// `/graphiql/main.js`. Whole-segment matching keeps a genuine REST resource such
/// as `/graphql-admin-tools` flagged.
fn is_graphql_path(path: &str) -> bool {
    path.split('/')
        .any(|seg| matches!(seg, "graphql" | "graphiql" | "playground"))
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

/// True when any `/`-delimited segment of `path` is a version token (`v1`,
/// `v2`, …). Unlike [`has_version_prefix`], the version may appear anywhere in
/// the path, not only as the leading segment — a sub-router is mounted at a
/// prefix such as `/-/npm/v1/security`, where the version sits mid-path.
fn contains_version_segment(path: &str) -> bool {
    path.split('/').any(|seg| {
        let Some(digits) = seg.strip_prefix('v') else {
            return false;
        };
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
    })
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

/// When `call` is a router-mount of the form `<recv>.use(<versionedPath>, <ident>)`
/// (Express `app.use('/v1', router)`), returns the mounted router identifier name.
/// The version segment may sit anywhere in the mount path (e.g. `/-/npm/v1/x`).
fn versioned_mounted_router<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Option<&'a str> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    if member.property.name.as_str() != "use" {
        return None;
    }
    let mount_path = call.arguments.first()?.as_expression()?;
    let Expression::StringLiteral(lit) = mount_path else {
        return None;
    };
    if !contains_version_segment(lit.value.as_str()) {
        return None;
    }
    call.arguments[1..].iter().find_map(|arg| match arg {
        Argument::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    })
}

/// Identifier name of a route registration's receiver (`router` in
/// `router.get(...)`), when the receiver is a bare identifier.
fn receiver_ident<'a>(member: &'a oxc_ast::ast::StaticMemberExpression<'a>) -> Option<&'a str> {
    match &member.object {
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if is_test_file(ctx.path) {
            return Vec::new();
        }

        // Benchmark / example / demo / scaffold scripts register dummy routes
        // that are never deployed — they have no API contract or versioning
        // concern. Reuses the central aux-dir classifier (`benchmarks/`,
        // `bench/`, `perf/`, `examples/`, …) shared across rules.
        if ctx.file.path_segments.in_aux_dir {
            return Vec::new();
        }

        // First pass: collect router identifiers mounted at a versioned path
        // (`app.use('/v1', router)`). Routes registered on such a sub-router
        // already carry the version at the mount point, so their relative paths
        // (`router.get('/users')`) must not be flagged. Matching is by
        // identifier name across the whole file, not lexical scope.
        let mut versioned_routers: Vec<&str> = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::CallExpression(call) = node.kind()
                && let Some(router) = versioned_mounted_router(call)
            {
                versioned_routers.push(router);
            }
        }

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            let name = member.property.name.as_str();
            if !ROUTE_METHODS.contains(&name) {
                continue;
            }
            if receiver_is_http_client(member) {
                continue;
            }
            if receiver_ident(member).is_some_and(|recv| versioned_routers.contains(&recv)) {
                continue;
            }

            let Some(first_arg) = call.arguments.first() else {
                continue;
            };
            let Some(first_expr) = first_arg.as_expression() else {
                continue;
            };
            let Some(route_path) = extract_route_path(first_expr, ctx.source) else {
                continue;
            };

            // Distinguish a server route registration from a client HTTP call.
            // Verb methods (`get`, `post`, …) are overloaded: a framework router
            // registers `app.get("/users", handler)` (handler after the path)
            // while an HTTP client requests `client.get("/users")` (path only).
            // The router-specific `route` method has no client counterpart, so
            // it stays a route regardless of its arguments.
            if name != "route" && !call.arguments[1..].iter().any(is_handler_arg) {
                continue;
            }
            if has_version_prefix(route_path)
                || is_infra_path(route_path)
                || is_oauth_oidc_path(route_path)
                || is_well_known_path(route_path)
                || is_graphql_path(route_path)
            {
                continue;
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
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
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

    /// Path-aware variant: builds a real `FileCtx` so the rule's
    /// `ctx.file.path_segments.in_aux_dir` bail is exercised from the path
    /// (the default `run` helper uses an empty `FileCtx`).
    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        use crate::files::Language;
        let path_ref = std::path::Path::new(path);
        let lang = Language::from_path(path_ref).unwrap_or(Language::TypeScript);
        let project = crate::project::default_static_project_ctx();
        let file = crate::rules::file_ctx::FileCtx::build(path_ref, source, lang, project);
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path_ref, project, &file)
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
    fn ignores_test_suite_factory_file() {
        // Issue #1661 — a `*Tests.ts` test-suite-factory file (apollo-server's
        // `apolloServerTests.ts`) spins up ephemeral Express servers for
        // integration testing; its unversioned routes are test scaffolding.
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "app.get('/', (req, res) => res.json({ ok: true }));",
            "packages/integration-testsuite/src/apolloServerTests.ts",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn flags_production_file_ending_in_lowercase_tests() {
        // Negative space: a real source file whose name merely ends in lowercase
        // `tests` (no capital boundary) is not a test factory and still flags.
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "app.get('/users', handler);",
            "src/manifests.ts",
        );
        assert_eq!(d.len(), 1);
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
    fn allows_underscore_health_probe() {
        // Issue #5372 — Strapi registers `router.all('/_health', healthCheck)`.
        // `/_health` (and other underscore-prefixed probe variants) are standard
        // liveness endpoints polled by orchestrators; they are unversioned by
        // convention and must not be flagged.
        assert!(run("router.all('/_health', healthCheck);").is_empty());
        assert!(run("app.get('/_ready', handler);").is_empty());
        assert!(run("app.get('/_live', handler);").is_empty());
    }

    #[test]
    fn allows_conventional_infra_endpoints() {
        // The standardized operational-route family: probe, scrape target, and
        // the conventional ping/status/version + crawler files.
        assert!(run("app.get('/ping', handler);").is_empty());
        assert!(run("app.get('/status', handler);").is_empty());
        assert!(run("app.get('/version', handler);").is_empty());
        assert!(run("app.get('/favicon.ico', handler);").is_empty());
        assert!(run("app.get('/robots.txt', handler);").is_empty());
    }

    #[test]
    fn flags_resource_route_resembling_infra_segment() {
        // Negative space: the infra exemption is segment-aware. A real API
        // resource whose name merely contains `health` as a substring is not a
        // probe and still requires a version prefix.
        assert_eq!(run("app.get('/healthcheck-config', handler);").len(), 1);
        assert_eq!(run("app.get('/status-reports', handler);").len(), 1);
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

    #[test]
    fn allows_oauth_oidc_protocol_routes() {
        // Issue #1777 — OAuth 2.0 (RFC 6749) / OIDC endpoint paths are mandated
        // by the spec; clients and registered redirect URIs expect them verbatim,
        // so they cannot carry a version prefix.
        assert!(run("routes.get('/authorize', async (c) => c.redirect(url));").is_empty());
        assert!(run("routes.get('/callback', async (c) => ctx.success(c, {}));").is_empty());
        assert!(run("routes.post('/token', async (c) => {});").is_empty());
        assert!(run("routes.get('/userinfo', handler);").is_empty());
        assert!(run("routes.get('/me', handler);").is_empty());
        assert!(run("routes.post('/introspect', handler);").is_empty());
        assert!(run("routes.post('/revoke', handler);").is_empty());
    }

    #[test]
    fn flags_unversioned_route_resembling_oauth_path() {
        // Conservative: only the exact protocol paths are exempt. A nested or
        // differently-named route still requires a version prefix.
        assert_eq!(run("app.get('/token/refresh', handler);").len(), 1);
        assert_eq!(run("app.get('/authorized', handler);").len(), 1);
    }

    #[test]
    fn allows_well_known_discovery_paths() {
        // Issue #3384 — `/.well-known/` is the IANA-reserved discovery namespace
        // (RFC 5785). Its sub-paths are registered at fixed standardized paths and
        // cannot carry a version prefix without breaking discovery.
        assert!(
            run("app.get('/.well-known/appspecific/com.chrome.devtools.json', (_, res) => res.send('x'));")
                .is_empty()
        );
        assert!(run("app.get('/.well-known/security.txt', handler);").is_empty());
        assert!(run("app.get('/.well-known/openid-configuration', handler);").is_empty());
        assert!(run("app.get('/.well-known/oauth-authorization-server', handler);").is_empty());
    }

    #[test]
    fn allows_graphql_endpoint() {
        // Issue #5381 — `/graphql` is the conventional single, unversioned GraphQL
        // endpoint; versioning happens in the schema, not the URL. The canonical
        // path, a mounted path, and a protocol sub-path are all exempt.
        assert!(run("app.post('/graphql', async (req, res) => {});").is_empty());
        assert!(run("app.post('/api/graphql', handler);").is_empty());
        assert!(run("app.post('/graphql/stream', handler);").is_empty());
        assert!(run("app.get('/graphql/subscriptions', handler);").is_empty());
    }

    #[test]
    fn allows_graphql_ide_routes() {
        // Issue #5591 — GraphiQL (`/graphiql`) and the `/playground` UI are the
        // GraphQL in-browser IDEs served at conventional unversioned paths; they
        // are static developer tools, not versioned resources. The IDE root and
        // its static asset sub-paths are all exempt.
        assert!(run("app.get('/graphiql', (req, reply) => {});").is_empty());
        assert!(run("app.get('/graphiql/main.js', (req, reply) => {});").is_empty());
        assert!(run("app.get('/graphiql/sw.js', (req, reply) => {});").is_empty());
        assert!(run("app.get('/graphiql/config.js', (req, reply) => {});").is_empty());
        assert!(run("app.get('/playground', handler);").is_empty());
    }

    #[test]
    fn flags_graphql_lookalike_resource() {
        // Negative space: whole-segment matching only — a real REST resource whose
        // segment merely starts with `graphql` is not the GraphQL endpoint and
        // still requires a version prefix.
        assert_eq!(run("app.get('/graphql-admin-tools', handler);").len(), 1);
        assert_eq!(run("app.get('/graphiql-settings', handler);").len(), 1);
        assert_eq!(run("app.get('/playgrounds', handler);").len(), 1);
    }

    #[test]
    fn flags_normal_versionless_route_alongside_well_known_exemption() {
        // Negative space: the `/.well-known/` exemption must not broaden to ordinary
        // unversioned API routes.
        assert_eq!(run("app.get('/users', handler);").len(), 1);
    }

    #[test]
    fn ignores_routes_in_benchmark_dir() {
        // Issue #3238 — router-performance benchmark scripts register dummy
        // routes (no handler logic, never deployed) to compare routing
        // throughput across frameworks. They have no API contract, so their
        // unversioned paths must not be flagged.
        assert!(run_at("app.get('/user', () => {})", "benchmarks/deno/faster.ts").is_empty());
        assert!(run_at("app.get('/user/comments', () => {})", "benchmarks/deno/faster.ts").is_empty());
        assert!(run_at("app.get('/event/:id', () => {})", "benchmarks/webapp/itty-router.js").is_empty());
    }

    #[test]
    fn flags_same_route_at_production_path() {
        // Load-bearing for #3238: the identical route at a production path still
        // requires a version prefix — the aux-dir bail must not leak to source.
        let d = run_at("app.get('/user', () => {})", "src/routes.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("/user"));
    }

    #[test]
    fn allows_subrouter_routes_mounted_at_versioned_path() {
        // Issue #4730 — routes registered on a sub-router are relative paths; the
        // version lives at the mount point. When the router is mounted at a
        // versioned path the relative routes must not be flagged.
        let src = "const router = express.Router();\n\
            router.post('/audits', express.json({ limit: '10mb' }), handleAudit);\n\
            router.post('/audits/quick', express.json({ limit: '10mb' }), handleAudit);\n\
            router.post('/advisories/bulk', express.json({ limit: '10mb' }), handleAudit);\n\
            app.use('/-/npm/v1/security', router);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_subrouter_mounted_at_leading_version() {
        // The version may also be the leading mount segment.
        let src = "const router = express.Router();\n\
            router.get('/users', handler);\n\
            app.use('/v1', router);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_subrouter_mounted_at_unversioned_path() {
        // Negative space: a sub-router mounted at an unversioned path gains no
        // version, so its relative routes still require a prefix.
        let src = "const router = express.Router();\n\
            router.get('/users', handler);\n\
            app.use('/-/npm/security', router);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unmounted_router_routes() {
        // Negative space: a router with no versioned mount in the file is
        // treated as a top-level route source and still flags.
        let src = "const router = express.Router();\n\
            router.get('/users', handler);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_app_level_route_despite_versioned_mount_of_other_router() {
        // The mount exemption is per-router: a versioned mount of `router` does
        // not exempt unversioned routes registered directly on `app`.
        let src = "const router = express.Router();\n\
            app.use('/v1', router);\n\
            app.get('/users', handler);";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("/users"));
    }
}
