//! OXC backend for elysia-test-missing-validation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

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

        let has_schema = ctx.source.contains("t.Object") || ctx.source.contains("body:");
        if !has_schema {
            return Vec::new();
        }

        let uses_elysia_client = ctx.source.contains("app.handle(")
            || ctx.source.contains("treaty(")
            || ctx.source.contains("new Elysia(");
        if !uses_elysia_client {
            return Vec::new();
        }

        // Static-analysis sweep tests inspect app.routes / app.stack — not HTTP responses.
        if ctx.source.contains("app.routes") || ctx.source.contains("app.stack") {
            return Vec::new();
        }

        if ctx.source.contains("400") || ctx.source.contains("422") {
            return Vec::new();
        }

        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Test declares a body schema but never asserts a 400/422 validation error."
                .into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}
