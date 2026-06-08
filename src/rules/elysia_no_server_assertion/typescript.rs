//! elysia-no-server-assertion backend — flag `server!` non-null assertions.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["non_null_expression"] prefilter = ["server!"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    // text looks like `something!` — check that it ends with `server!` or `.server!`.
    if !(text.ends_with(".server!") || text == "server!") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-no-server-assertion".into(),
        message: "`server!` non-null assertion is unsafe — `app.server` is undefined until `.listen()` resolves.".into(),
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
    fn flags_app_server_bang() {
        let src = "import { Elysia } from 'elysia';\nconst port = app.server!.port;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_bare_server_bang() {
        let src = "import { Elysia } from 'elysia';\nconst s = server!;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_server_without_bang() {
        let src = "import { Elysia } from 'elysia';\napp.listen(3000, () => { console.log(app.server?.port); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const port = app.server!.port;";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
