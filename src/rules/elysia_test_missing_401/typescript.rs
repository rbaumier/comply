//! elysia-test-missing-401 backend — auth tests must assert 401.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

/// Auth middleware identifiers — matched verbatim.
const AUTH_INVOCATION_MARKERS: &[&str] = &[
    "requireAuth",
    "requireAuthorization",
    "authPlugin",
    "authMiddleware",
    "betterAuth",
];

/// Auth invocation patterns — matched after lowercasing.
const AUTH_INVOCATION_MARKERS_CI: &[&str] = &[
    ".use(auth",
    "beforehandle: requireauth",
    "beforehandle: auth",
    "bearer(",
    "jwt(",
];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let _ = (node, source);
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !is_test_file(ctx.path) {
        return;
    }

    let lower = ctx.source.to_lowercase();

    // Bare "auth" in header literals is not middleware composition.
    let exercises_auth_route = AUTH_INVOCATION_MARKERS
        .iter()
        .any(|m| ctx.source.contains(m))
        || AUTH_INVOCATION_MARKERS_CI
            .iter()
            .any(|m| lower.contains(m));
    if !exercises_auth_route {
        return;
    }

    let tests_http_routes = ctx.source.contains(".handle(")
        || lower.contains("treaty(")
        || lower.contains("supertest")
        || ctx.source.contains("\"/api/")
        || ctx.source.contains("'/api/");
    if !tests_http_routes {
        return;
    }

    if ctx.source.contains("401") || lower.contains("unauthorized") {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: 1,
        column: 1,
        rule_id: "elysia-test-missing-401".into(),
        message: "Test exercises an authenticated route but never asserts a 401/Unauthorized response.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on_test(source: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::for_test_with_framework("elysia");
        crate::rules::test_helpers::run_ts_with_project_and_path(
            source,
            &Check,
            &project,
            std::path::Path::new("auth.test.ts"),
        )
    }

    #[test]
    fn flags_auth_test_without_401() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().use(bearer()).get('/me', requireAuth, () => 'ok');\ntest('bearer token works', () => { const r = app.handle(new Request('/me')); });";
        assert_eq!(run_on_test(src).len(), 1);
    }

    #[test]
    fn allows_auth_test_with_401_assertion() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().use(bearer()).get('/me', requireAuth);\ntest('bearer rejects', () => { expect(r.status).toBe(401); });";
        assert!(run_on_test(src).is_empty());
    }

    #[test]
    fn ignores_non_auth_test() {
        let src = "import { Elysia } from 'elysia';\ntest('list users', () => { expect(r.status).toBe(200); });";
        assert!(run_on_test(src).is_empty());
    }

    #[test]
    fn still_flags_capitalized_bearer_invocation() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().use(Bearer()).get('/', requireAuth(), () => 'x');\ntest('auth required', () => { const r = app.handle(new Request('/')); });";
        assert_eq!(run_on_test(src).len(), 1);
    }

    /// Regression for rbaumier/comply#92 — CORS exposeHeaders test that puts
    /// `Authorization` / `Cookie` in the request fixture as header noise must
    /// not be flagged, because the test never composes an auth-guarded route.
    #[test]
    fn ignores_cors_test_with_auth_headers_in_fixture() {
        let src = r#"import { coreMiddlewarePlugin } from './composition';
test('exposes only allowlisted headers', async () => {
  const app = coreMiddlewarePlugin({ config, errorReporter: { capture() {} } });
  const res = await app.handle(new Request('http://example.test:3000/', {
    method: 'GET',
    headers: { Origin: 'https://x', Cookie: 'stub', Authorization: 'stub' },
  }));
  const raw = res.headers.get('access-control-expose-headers');
  expect(raw.split(',')).toEqual(['x-request-id', 'ratelimit-limit']);
});
"#;
        assert!(run_on_test(src).is_empty());
    }
}
