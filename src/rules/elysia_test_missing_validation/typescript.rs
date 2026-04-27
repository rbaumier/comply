//! elysia-test-missing-validation backend — validation tests must assert 400/422.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

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

    let has_schema = ctx.source.contains("t.Object") || ctx.source.contains("body:");
    if !has_schema {
        return;
    }

    if ctx.source.contains("400") || ctx.source.contains("422") {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: 1,
        column: 1,
        rule_id: "elysia-test-missing-validation".into(),
        message: "Test declares a body schema but never asserts a 400/422 validation error.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on_test(source: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::for_test_with_framework("elysia");
        crate::rules::test_helpers::run_ts_with_project_and_path(source, &Check, &project, std::path::Path::new("create.test.ts"))
    }

    #[test]
    fn flags_schema_without_400() {
        let src = "import { Elysia, t } from 'elysia';\ntest('create', () => { app.post('/x', () => {}, { body: t.Object({ a: t.String() }) }); expect(r.status).toBe(200); });";
        assert_eq!(run_on_test(src).len(), 1);
    }

    #[test]
    fn allows_schema_with_400() {
        let src = "import { Elysia, t } from 'elysia';\ntest('create rejects', () => { app.post('/x', () => {}, { body: t.Object({ a: t.String() }) }); expect(r.status).toBe(400); });";
        assert!(run_on_test(src).is_empty());
    }

    #[test]
    fn ignores_test_without_schema() {
        let src = "import { Elysia } from 'elysia';\ntest('hello', () => { expect(r.status).toBe(200); });";
        assert!(run_on_test(src).is_empty());
    }
}
