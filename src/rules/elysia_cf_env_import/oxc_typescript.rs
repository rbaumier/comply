//! OXC backend for elysia-cf-env-import — flag `process.env` under CloudflareAdapter.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

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

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@cloudflare/", "cloudflare:workers", "elysia/adapter/cloudflare"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StaticMemberExpression(expr) = node.kind() else { return };

        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !is_cloudflare_context(ctx.source, ctx.path) {
            return;
        }

        let text = &ctx.source[expr.span.start as usize..expr.span.end as usize];
        if !text.starts_with("process.env") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "elysia-cf-env-import".into(),
            message: "`process.env` is undefined on Cloudflare Workers — use `import { env } from 'cloudflare:workers'`.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
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
        let src = "import { Elysia } from 'elysia';\nconst secret = process.env.SECRET;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_cloudflare_workers_import() {
        let src = "import { env } from 'cloudflare:workers';\nconst leak = process.env.SECRET;";
        assert!(!run_on(src).is_empty());
    }
}
