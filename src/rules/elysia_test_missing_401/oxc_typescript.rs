//! elysia-test-missing-401 oxc backend — auth tests must assert 401.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

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

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }
        if !is_test_file(ctx.path) {
            return Vec::new();
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
            return Vec::new();
        }

        let tests_http_routes = ctx.source_contains(".handle(")
            || lower.contains("treaty(")
            || lower.contains("supertest")
            || ctx.source_contains("\"/api/")
            || ctx.source_contains("'/api/");
        if !tests_http_routes {
            return Vec::new();
        }

        if has_composition_concern(ctx.path, ctx.source) {
            return Vec::new();
        }

        if ctx.source_contains("401") || lower.contains("unauthorized") {
            return Vec::new();
        }

        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Test exercises an authenticated route but never asserts a 401/Unauthorized response.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}
