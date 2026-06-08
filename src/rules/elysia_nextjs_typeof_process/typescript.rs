//! elysia-nextjs-typeof-process backend — flag `typeof window` in eden treaty isomorphic clients.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["unary_expression"] prefilter = ["@elysiajs/eden"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !ctx.source_contains("@elysiajs/eden") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    if !text.starts_with("typeof window") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-nextjs-typeof-process".into(),
        message: "Use `typeof process` instead of `typeof window` — `window` checks misclassify edge / RSC runtimes.".into(),
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
    fn flags_typeof_window_in_treaty_file() {
        let src = "import { treaty } from '@elysiajs/eden';\nconst isServer = typeof window === 'undefined';";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_typeof_process() {
        let src = "import { treaty } from '@elysiajs/eden';\nconst isServer = typeof process !== 'undefined';";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_eden_files() {
        let src = "const isServer = typeof window === 'undefined';";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
