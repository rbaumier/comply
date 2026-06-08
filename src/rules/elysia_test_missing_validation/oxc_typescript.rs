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

        let has_schema = ctx.source_contains("t.Object") || ctx.source_contains("body:");
        if !has_schema {
            return Vec::new();
        }

        let uses_elysia_client = ctx.source_contains("app.handle(")
            || ctx.source_contains("treaty(")
            || ctx.source_contains("new Elysia(");
        if !uses_elysia_client {
            return Vec::new();
        }

        // Static-analysis sweep tests inspect app.routes / app.stack — not HTTP responses.
        if ctx.source_contains("app.routes") || ctx.source_contains("app.stack") {
            return Vec::new();
        }

        if ctx.source_contains("400") || ctx.source_contains("422") {
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

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on_test(source: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::for_test_with_framework("elysia");
        crate::rules::test_helpers::run_oxc_ts_with_project(
            source,
            &Check,
            &project)
    }


    #[test]
    fn allows_schema_with_400() {
        let src = "import { Elysia, t } from 'elysia';\nconst app = new Elysia();\ntest('create rejects', async () => { const r = await app.handle(new Request('/x', { method: 'POST', body: '{}' })); expect(r.status).toBe(400); });\n// body: t.Object({ a: t.String() })";
        assert!(run_on_test(src).is_empty());
    }


    #[test]
    fn ignores_test_without_schema() {
        let src = "import { Elysia } from 'elysia';\ntest('hello', () => { expect(r.status).toBe(200); });";
        assert!(run_on_test(src).is_empty());
    }


    #[test]
    fn ignores_static_analysis_sweep_without_elysia_client() {
        // Regression for #105: filesystem-sweep tests over handler source files
        // mention `body:` in strings/comments but never make Elysia requests.
        let src = "describe('no inline z.object in Elysia .body() / .response()', () => {\n  it('every handler wires the canonical schema', async () => {\n    const offenders = await findAllOffenders();\n    expect(offenders).toEqual([]);\n  });\n});";
        assert!(run_on_test(src).is_empty());
    }


    #[test]
    fn no_fp_on_app_routes_sweep() {
        // Regression for #367: tests iterating app.routes to verify schema presence
        // are static-analysis sweeps, not HTTP request tests.
        let src = "import { app } from '../app';\nimport { Elysia, t } from 'elysia';\nconst _app = new Elysia();\nit('all routes have validation schemas', () => {\n  const routes = app.routes;\n  for (const route of routes) {\n    expect(route.schema).toBeDefined();\n  }\n});\n// body: t.Object({ a: t.String() })";
        assert!(run_on_test(src).is_empty());
    }


    #[test]
    fn no_fp_on_app_stack_sweep() {
        // Regression for #367: tests iterating app.stack also count as static-analysis sweeps.
        let src = "import { app } from '../app';\nimport { Elysia, t } from 'elysia';\nconst _app = new Elysia();\nit('all handlers have body validation', () => {\n  const stack = app.stack;\n  for (const entry of stack) {\n    expect(entry.hooks.body).toBeDefined();\n  }\n});\n// body: t.Object({ a: t.String() })";
        assert!(run_on_test(src).is_empty());
    }
}
