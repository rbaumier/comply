//! elysia-deploy-no-health backend — flag Elysia servers that listen without exposing `/health`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = [".listen"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !ctx.source_contains(".listen(") {
        return;
    }
    // Accept any conventional liveness/readiness probe path. `/health` is the
    // common one but Kubernetes apps frequently expose `/healthz`, `/readyz`,
    // `/livez`, `/_health`, or split into `/health/live` and `/health/ready`.
    const HEALTH_PATHS: &[&str] = &[
        "/health",
        "/healthz",
        "/readyz",
        "/livez",
        "/_health",
        "/health/live",
        "/health/ready",
    ];
    if HEALTH_PATHS.iter().any(|p| ctx.source_contains(p)) {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".listen") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-deploy-no-health".into(),
        message: "Elysia server exposes `.listen()` without a `/health` endpoint — orchestrators lack a liveness probe.".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_listen_without_health() {
        let src =
            "import { Elysia } from 'elysia';\nnew Elysia().get('/users', () => []).listen(3000);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_listen_with_health_route() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/health', () => 'ok').listen(3000);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.listen(3000);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn allows_listen_with_healthz() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/healthz', () => 'ok').listen(3000);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_listen_with_readyz() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/readyz', () => 'ok').listen(3000);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_listen_with_livez() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/livez', () => 'ok').listen(3000);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_listen_with_underscore_health() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/_health', () => 'ok').listen(3000);";
        assert!(run_on(src).is_empty());
    }
}
