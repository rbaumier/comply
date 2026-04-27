//! elysia-test-missing-401 backend — auth tests must assert 401.

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

    let lower = ctx.source.to_lowercase();
    let touches_auth = lower.contains("auth") || lower.contains("bearer") || lower.contains("jwt");
    if !touches_auth {
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
        crate::rules::test_helpers::run_ts_with_project_and_path(source, &Check, &project, std::path::Path::new("auth.test.ts"))
    }

    #[test]
    fn flags_auth_test_without_401() {
        let src = "import { Elysia } from 'elysia';\ntest('bearer token works', () => { const r = app.handle(new Request('/me', { headers: { authorization: 'Bearer x' } })); });";
        assert_eq!(run_on_test(src).len(), 1);
    }

    #[test]
    fn allows_auth_test_with_401_assertion() {
        let src = "import { Elysia } from 'elysia';\ntest('bearer rejects', () => { expect(r.status).toBe(401); });";
        assert!(run_on_test(src).is_empty());
    }

    #[test]
    fn ignores_non_auth_test() {
        let src = "import { Elysia } from 'elysia';\ntest('list users', () => { expect(r.status).toBe(200); });";
        assert!(run_on_test(src).is_empty());
    }
}
