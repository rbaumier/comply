//! elysia-static-await-hmr backend — flag `staticPlugin()` without `await`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "staticPlugin" {
        return;
    }

    // Check whether the parent of this call is an await_expression.
    if let Some(parent) = node.parent() {
        if parent.kind() == "await_expression" {
            return;
        }
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-static-await-hmr".into(),
        message: "`staticPlugin()` is async — use `await staticPlugin()` so HMR picks up file changes.".into(),
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
    fn flags_use_static_plugin_without_await() {
        let src = "import { Elysia } from 'elysia';\nimport { staticPlugin } from '@elysiajs/static';\nnew Elysia().use(staticPlugin());";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_use_static_plugin_with_await() {
        let src = "import { Elysia } from 'elysia';\nimport { staticPlugin } from '@elysiajs/static';\nasync function main() { return new Elysia().use(await staticPlugin()); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_files_without_static_import() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().use(staticPlugin());";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
