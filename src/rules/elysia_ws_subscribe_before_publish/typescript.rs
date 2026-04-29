//! elysia-ws-subscribe-before-publish backend — flag `.publish(` with no `.subscribe(`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = [".publish"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !ctx.source.contains(".ws(") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".publish") {
        return;
    }

    if ctx.source.contains(".subscribe(") {
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
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
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
