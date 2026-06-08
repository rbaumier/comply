//! elysia-cf-no-static-plugin oxc backend — flag `staticPlugin` or `.file(`
//! under CloudflareAdapter.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

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
        crate::rules::test_helpers::run_oxc_tsx_with_project(source, &Check, project)
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
    fn flags_when_wrangler_toml_present() {
        let dir = TempDir::new().unwrap();
        let project = cf_project(dir.path());
        let src = "import { staticPlugin } from '@elysiajs/static';\napp.use(staticPlugin());";
        assert_eq!(run_in_project(src, &project).len(), 1);
    }

    #[test]
    fn flags_when_wrangler_jsonc_present() {
        let dir = TempDir::new().unwrap();
        let project = cf_project_with_marker(dir.path(), "wrangler.jsonc");
        let src = "import { staticPlugin } from '@elysiajs/static';\napp.use(staticPlugin());";
        assert_eq!(run_in_project(src, &project).len(), 1);
    }



    #[test]
    fn ignores_non_cf_files() {
        let src = "import { staticPlugin } from '@elysiajs/static';\napp.use(staticPlugin());";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["staticPlugin"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !ctx.project.is_cloudflare_target() {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        let last = callee_text.rsplit('.').next().unwrap_or("");
        let is_static_plugin = callee_text == "staticPlugin";
        let is_file = last == "file" && callee_text.contains('.');

        if !is_static_plugin && !is_file {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Filesystem-backed static serving (`staticPlugin` / `.file()`) does not work under Cloudflare — use the `[assets]` binding.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
