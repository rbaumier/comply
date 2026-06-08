//! elysia-eden-server-export-type backend — flag server files without `export type`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let _ = (node, source);
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !ctx.source_contains("new Elysia(") {
        return;
    }
    if !ctx.source_contains(".listen(") {
        return;
    }
    if ctx.source_contains("export type") {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: 1,
        column: 1,
        rule_id: "elysia-eden-server-export-type".into(),
        message: "Server entry has no `export type` — Eden Treaty cannot infer routes from this module.".into(),
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
    fn flags_server_without_export_type() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().get('/', () => 'hi').listen(3000);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_server_with_export_type() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().get('/', () => 'hi').listen(3000);\nexport type App = typeof app;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_server_files() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia().get('/', () => 'hi');";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
