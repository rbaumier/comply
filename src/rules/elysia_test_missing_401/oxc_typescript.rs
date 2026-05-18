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

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
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
            .any(|m| ctx.source.contains(m))
            || AUTH_INVOCATION_MARKERS_CI
                .iter()
                .any(|m| lower.contains(m));
        if !exercises_auth_route {
            return Vec::new();
        }

        let tests_http_routes = ctx.source.contains(".handle(")
            || lower.contains("treaty(")
            || lower.contains("supertest")
            || ctx.source.contains("\"/api/")
            || ctx.source.contains("'/api/");
        if !tests_http_routes {
            return Vec::new();
        }

        if ctx.source.contains("401") || lower.contains("unauthorized") {
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
