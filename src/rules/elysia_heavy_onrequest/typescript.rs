//! elysia-heavy-onrequest backend — flag heavy work inside `.onRequest(`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".onRequest") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    let heavy = args_text.contains("await ")
        || args_text.contains("fetch(")
        || args_text.contains("db.")
        || args_text.contains("prisma.")
        || args_text.contains("JSON.parse");
    if !heavy {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-heavy-onrequest".into(),
        message: "`.onRequest()` runs before routing on every request — move heavy work (await/fetch/db/JSON.parse) to `.beforeHandle()`.".into(),
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
    fn flags_await_in_on_request() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onRequest(async ({ request }) => { await fetch('/x'); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_db_in_on_request() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onRequest(({ request }) => { db.query('SELECT 1'); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_lightweight_on_request() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onRequest(({ request }) => { console.log(request.url); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.onRequest(async () => { await fetch('/x'); });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
