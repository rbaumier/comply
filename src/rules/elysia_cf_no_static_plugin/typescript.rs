//! elysia-cf-no-static-plugin backend — flag `staticPlugin` or `.file(` under `CloudflareAdapter`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["staticPlugin"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !ctx.project.is_cloudflare_target() {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    let last = callee_text.rsplit('.').next().unwrap_or("");
    let is_static_plugin = callee_text == "staticPlugin";
    let is_file = last == "file" && callee_text.contains('.');
    if !is_static_plugin && !is_file {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cf-no-static-plugin".into(),
        message: "Filesystem-backed static serving (`staticPlugin` / `.file()`) does not work under Cloudflare — use the `[assets]` binding.".into(),
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
    use crate::project::ProjectCtx;
    use std::path::Path;
    use tempfile::TempDir;

    fn cf_project_with_marker(dir: &Path, marker: &str) -> ProjectCtx {
        std::fs::write(dir.join(marker), "name = \"x\"\n").unwrap();
        let mut ctx = ProjectCtx::for_test_with_framework("elysia");
        ctx.project_root = Some(dir.to_path_buf());
        ctx
    }

    fn cf_project(dir: &Path) -> ProjectCtx {
        cf_project_with_marker(dir, "wrangler.toml")
    }

    fn non_cf_project(dir: &Path) -> ProjectCtx {
        let mut ctx = ProjectCtx::for_test_with_framework("elysia");
        ctx.project_root = Some(dir.to_path_buf());
        ctx
    }

    fn run_in_project(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, Path::new("t.ts"), project, crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_static_plugin() {
        let dir = TempDir::new().unwrap();
        let project = cf_project(dir.path());
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\nimport { staticPlugin } from '@elysiajs/static';\napp.use(staticPlugin());";
        assert_eq!(run_in_project(src, &project).len(), 1);
    }

    #[test]
    fn flags_file_helper() {
        let dir = TempDir::new().unwrap();
        let project = cf_project(dir.path());
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\napp.get('/img', ({ file }) => file('logo.png'));\napp.file('img.png');";
        assert!(!run_in_project(src, &project).is_empty());
    }

    #[test]
    fn ignores_non_cf_files() {
        let src = "import { staticPlugin } from '@elysiajs/static';\napp.use(staticPlugin());";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn ignores_bun_k8s_project_without_wrangler() {
        let dir = TempDir::new().unwrap();
        let project = non_cf_project(dir.path());
        let src = "import { staticPlugin } from \"@elysiajs/static\";\nconst staticApp = new Elysia().use(\n  staticPlugin({ assets: \"dist/client/assets\", prefix: \"/assets\" })\n);";
        assert!(
            run_in_project(src, &project).is_empty(),
            "Bun + K8s projects without a wrangler.toml must not be flagged"
        );
    }

    #[test]
    fn flags_when_wrangler_jsonc_present() {
        let dir = TempDir::new().unwrap();
        let project = cf_project_with_marker(dir.path(), "wrangler.jsonc");
        let src = "import { staticPlugin } from '@elysiajs/static';\napp.use(staticPlugin());";
        assert_eq!(run_in_project(src, &project).len(), 1);
    }
}
