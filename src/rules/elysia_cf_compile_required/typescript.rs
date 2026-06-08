//! elysia-cf-compile-required backend — `CloudflareAdapter` requires `.compile()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["identifier"] prefilter = [".compile()"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if ctx.source_contains(".compile()") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    if text != "CloudflareAdapter" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cf-compile-required".into(),
        message: "Elysia under `CloudflareAdapter` must call `.compile()` before export.".into(),
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
    fn flags_cf_without_compile() {
        let src = "import { Elysia } from 'elysia';\nimport { CloudflareAdapter } from 'elysia/adapter/cloudflare';\nexport default new Elysia({ adapter: CloudflareAdapter() });";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn allows_cf_with_compile() {
        let src = "import { Elysia } from 'elysia';\nimport { CloudflareAdapter } from 'elysia/adapter/cloudflare';\nexport default new Elysia({ adapter: CloudflareAdapter() }).get('/', () => 'hi').compile();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_cf_files() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().listen(3000);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
