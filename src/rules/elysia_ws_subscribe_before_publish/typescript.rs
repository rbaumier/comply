//! elysia-ws-subscribe-before-publish backend — flag `.publish(` with no `.subscribe(`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = [".publish"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !ctx.source_contains(".ws(") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".publish") {
        return;
    }

    if ctx.source_contains(".subscribe(") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-ws-subscribe-before-publish".into(),
        message: "`ws.publish()` is called but no client is `subscribe()`d to the topic — messages will be dropped.".into(),
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
    fn flags_publish_without_subscribe() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().ws('/chat', { message(ws, msg) { ws.publish('room', msg); } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_publish_with_subscribe() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().ws('/chat', { open(ws) { ws.subscribe('room'); }, message(ws, msg) { ws.publish('room', msg); } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "ws.publish('room', msg);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
