//! elysia-cf-env-import backend — flag `process.env` under `CloudflareAdapter`.
//!
//! Only fires on files that are clearly Cloudflare Workers code: they import
//! from `cloudflare:workers`, `@cloudflare/...`, or `elysia/adapter/cloudflare`,
//! or live under a `workers/` directory. On Bun/Node-only Elysia projects,
//! `process.env` is the correct API and must not be flagged.

use crate::diagnostic::{Diagnostic, Severity};

fn is_cloudflare_context(source: &str, path: &std::path::Path) -> bool {
    if crate::oxc_helpers::source_contains(source, "'cloudflare:workers'")
        || crate::oxc_helpers::source_contains(source, "\"cloudflare:workers\"")
        || crate::oxc_helpers::source_contains(source, "'@cloudflare/")
        || crate::oxc_helpers::source_contains(source, "\"@cloudflare/")
        || crate::oxc_helpers::source_contains(source, "'elysia/adapter/cloudflare'")
        || crate::oxc_helpers::source_contains(source, "\"elysia/adapter/cloudflare\"")
        || crate::oxc_helpers::source_contains(source, "CloudflareAdapter")
    {
        return true;
    }
    path.components()
        .any(|c| matches!(c.as_os_str().to_str(), Some("workers")))
}

crate::ast_check! { on ["member_expression"] prefilter = ["@cloudflare/", "cloudflare:workers", "elysia/adapter/cloudflare"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !is_cloudflare_context(ctx.source, ctx.path) {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    if !text.starts_with("process.env") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cf-env-import".into(),
        message: "`process.env` is undefined on Cloudflare Workers — use `import { env } from 'cloudflare:workers'`.".into(),
        severity: Severity::Error,
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
    fn flags_process_env_under_cf() {
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\nconst secret = process.env.SECRET;";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn allows_cloudflare_workers_env() {
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\nimport { env } from 'cloudflare:workers';\nconst secret = env.SECRET;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_cf_files() {
        let src = "const secret = process.env.SECRET;";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn ignores_bun_elysia_without_cf_import() {
        // Regression: Elysia on Bun has access to `process.env`. The rule
        // must not fire just because the project uses Elysia — only when the
        // file actually targets Cloudflare Workers.
        let src = "import { Elysia } from 'elysia';\nconst secret = process.env.SECRET;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_vite_config_in_elysia_project() {
        // Regression: Elysia projects often ship a Vite-bundled frontend whose
        // `vite.config.ts` legitimately reads `process.env`.
        let src = "import { defineConfig } from 'vite';\nexport default defineConfig({ define: { 'process.env.X': process.env.X } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_cloudflare_workers_import() {
        let src = "import { env } from 'cloudflare:workers';\nconst leak = process.env.SECRET;";
        assert!(!run_on(src).is_empty());
    }
}
