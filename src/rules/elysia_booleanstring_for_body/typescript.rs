//! elysia-booleanstring-for-body backend — `t.Boolean()` inside a body schema
//! rejects string-encoded booleans common in form submissions.

use crate::diagnostic::{Diagnostic, Severity};

const STOP_KEYS: &[&str] = &["params:", "query:", "headers:", "response:", "cookie:", "detail:", "tags:"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" { return; }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let prop_text = prop.utf8_text(source).unwrap_or("");
    const ROUTE_METHODS: &[&str] = &["post", "put", "patch", "delete"];
    if !ROUTE_METHODS.contains(&prop_text) { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

    let Some(body_idx) = norm.find("body:t.") else { return };
    let after_body = &norm[body_idx..];
    let cut = STOP_KEYS
        .iter()
        .filter_map(|k| after_body[1..].find(k).map(|i| i + 1))
        .min()
        .unwrap_or(after_body.len());
    let body_section = &after_body[..cut];

    if !body_section.contains("t.Boolean(") { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-booleanstring-for-body".into(),
        message: "`t.Boolean()` in a `body:` schema rejects `\"true\"`/`\"false\"` — use `t.BooleanString()` for form-encoded payloads.".into(),
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
    fn flags_boolean_in_body() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ active: t.Boolean() }) });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_boolean_string_in_body() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ active: t.BooleanString() }) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_boolean_in_response() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ name: t.String() }), response: { 200: t.Object({ ok: t.Boolean() }) } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.post('/x', () => 1, { body: t.Object({ active: t.Boolean() }) });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
