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

/// Keywords that indicate the test exercises a cross-cutting concern (CORS,
/// rate-limiting, request-ID propagation, etc.) rather than auth itself.
/// Matched case-insensitively against the file path and describe/test descriptions.
const COMPOSITION_CONCERN_MARKERS: &[&str] = &[
    "cors",
    "rate-limit",
    "ratelimit",
    "rate_limit",
    "composition",
    "request-id",
    "requestid",
    "x-request-id",
    "logging",
];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

fn has_composition_concern(path: &std::path::Path, source: &str) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();
    if COMPOSITION_CONCERN_MARKERS
        .iter()
        .any(|m| path_str.contains(m))
    {
        return true;
    }
    source
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("describe(")
                || t.starts_with("test(")
                || t.starts_with("it(")
        })
        .any(|l| {
            let lower = l.to_lowercase();
            COMPOSITION_CONCERN_MARKERS.iter().any(|m| lower.contains(m))
        })
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
        .any(|m| ctx.source_contains(m))
        || AUTH_INVOCATION_MARKERS_CI
            .iter()
            .any(|m| lower.contains(m));
    if !exercises_auth_route {
        return;
    }

    let tests_http_routes = ctx.source_contains(".handle(")
        || lower.contains("treaty(")
        || lower.contains("supertest")
        || ctx.source_contains("\"/api/")
        || ctx.source_contains("'/api/");
    if !tests_http_routes {
        return;
    }

    if has_composition_concern(ctx.path, ctx.source) {
        return;
    }

    if ctx.source_contains("401") || lower.contains("unauthorized") {
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on_test(source: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::for_test_with_framework("elysia");
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, std::path::Path::new("auth.test.ts"), &project, crate::rules::file_ctx::default_static_file_ctx())
    }

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::for_test_with_framework("elysia");
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, std::path::Path::new(path), &project, crate::rules::file_ctx::default_static_file_ctx())
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

    /// Regression for rbaumier/comply#360 — composition test file whose path
    /// contains "cors": full middleware stack includes authPlugin but the test
    /// asserts CORS behaviour only.
    #[test]
    fn ignores_authplugin_composition_when_cors_in_path() {
        let src = r#"import { authPlugin } from './auth';
import { corsPlugin } from './cors';
describe('cors headers', () => {
  test('includes access-control-allow-origin', async () => {
    const app = new Elysia().use(authPlugin()).use(corsPlugin({ origin: 'https://example.com' }));
    const res = await app.handle(new Request('http://localhost/health'));
    expect(res.headers.get('access-control-allow-origin')).toBe('https://example.com');
  });
});
"#;
        assert!(run_on_path(src, "cors.test.ts").is_empty());
    }

    /// Regression for rbaumier/comply#360 — betterAuth in composition but
    /// describe label says "cors headers": not an auth test.
    #[test]
    fn ignores_betterauth_when_describe_says_cors() {
        let src = r#"import { betterAuth } from 'better-auth';
describe('cors headers propagation', () => {
  test('exposes ratelimit headers', async () => {
    const app = new Elysia().use(betterAuth()).get('/ping', () => 'ok');
    const res = await app.handle(new Request('http://localhost/ping'));
    expect(res.headers.get('access-control-expose-headers')).toContain('ratelimit-limit');
  });
});
"#;
        assert!(run_on_test(src).is_empty());
    }

    /// Regression for rbaumier/comply#360 — authPlugin present in composition
    /// but test description mentions "rate-limit": not an auth test.
    #[test]
    fn ignores_authplugin_when_test_says_rate_limit() {
        let src = r#"import { authPlugin } from './auth';
test('rate-limit headers are present on all responses', async () => {
  const app = new Elysia().use(authPlugin()).get('/ping', () => 'ok');
  const res = await app.handle(new Request('http://localhost/ping'));
  expect(res.headers.get('ratelimit-limit')).toBeTruthy();
});
"#;
        assert!(run_on_test(src).is_empty());
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
