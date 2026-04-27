//! elysia-response-keyed-by-status backend — `response: t.X(...)` (no status
//! keying) hides error variants from the typed client.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" { return; }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let prop_text = prop.utf8_text(source).unwrap_or("");
    if !ROUTE_METHODS.contains(&prop_text) { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

    // `response:t.` indicates a bare TypeBox schema (no status keying).
    if !norm.contains("response:t.") { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-response-keyed-by-status".into(),
        message: "Use a status-keyed response: `response: { 200: t.Object({...}), 4xx: ... }` so error shapes are typed.".into(),
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
    fn flags_bare_response_typebox() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => ({}), { response: t.Object({ ok: t.Boolean() }) });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_status_keyed_response() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => ({}), { response: { 200: t.Object({ ok: t.Boolean() }) } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_route_without_response() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/x', () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/x', () => 'ok', { response: t.Object({}) });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
