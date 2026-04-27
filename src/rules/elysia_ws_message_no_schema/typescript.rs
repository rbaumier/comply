//! elysia-ws-message-no-schema — flag `.ws('/path', { body: ... })` calls
//! whose options object has `body:` but no `message:`.

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
    let text = args.utf8_text(source).unwrap_or("");
    let has_body = text.contains("body:") || text.contains("body :");
    let has_message = text.contains("message:") || text.contains("message :");
    if !has_body || has_message {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-ws-message-no-schema".into(),
        message: "WebSocket route declares `body:` but no `message:` — incoming frames are not validated.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_ws_with_body_no_message() {
        let src = "import { Elysia, t } from 'elysia';\napp.ws('/chat', { body: t.Object({}), open: () => {} });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_ws_with_message_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.ws('/chat', { body: t.Object({}), message: t.String() });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_ws_without_body() {
        let src = "import { Elysia } from 'elysia';\napp.ws('/chat', { open: () => {} });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.ws('/chat', { body: t.Object({}) });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
