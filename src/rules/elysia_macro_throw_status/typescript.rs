//! elysia-macro-throw-status backend — flag `throw status(...)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["throw_statement"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    let norm: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if !norm.contains("throwstatus(") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-macro-throw-status".into(),
        message: "Use `return status(...)` instead of `throw status(...)` so Elysia tracks the response type.".into(),
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
    fn flags_throw_status() {
        let src =
            "import { Elysia, status } from 'elysia';\nfunction guard() { throw status(401); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_return_status() {
        let src =
            "import { Elysia, status } from 'elysia';\nfunction guard() { return status(401); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "function guard() { throw status(401); }";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
