//! elysia-ws-connection-leak backend — flag `.ws()` configs that add to a Set in `open` but don't clean up on error/close.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "ws" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

    // Need an `open(` handler that does `.add(`.
    if !norm.contains("open(") && !norm.contains("open:") {
        return;
    }
    if !args_text.contains(".add(") {
        return;
    }

    // No error handler, or error handler exists but lacks `.delete(`.
    let has_error = norm.contains("error(") || norm.contains("error:");
    let cleans_up = args_text.contains(".delete(");

    if has_error && cleans_up {
        return;
    }

    let pos = node.start_position();
    let msg = if !has_error {
        "`.ws()` `open` adds to a Set but no `error` handler is defined — dead sockets leak."
    } else {
        "`.ws()` `open` adds to a Set but `error`/`close` does not call `.delete(ws)` — dead sockets leak."
    };
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-ws-connection-leak".into(),
        message: msg.into(),
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
    fn flags_open_add_without_error_handler() {
        let src = "import { Elysia } from 'elysia';\nconst peers = new Set();\napp.ws('/chat', { open(ws) { peers.add(ws); } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_error_without_delete() {
        let src = "import { Elysia } from 'elysia';\nconst peers = new Set();\napp.ws('/chat', { open(ws) { peers.add(ws); }, error(ws) { console.log('err'); } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_error_with_delete() {
        let src = "import { Elysia } from 'elysia';\nconst peers = new Set();\napp.ws('/chat', { open(ws) { peers.add(ws); }, error(ws) { peers.delete(ws); } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.ws('/chat', { open(ws) { peers.add(ws); } });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
