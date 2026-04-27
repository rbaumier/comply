//! elysia-route-missing-response-schema backend — when a route validates its
//! input via `body:` or `params:`, also expect a `response:` declaration.

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

    let validates_input = norm.contains("body:") || norm.contains("params:");
    if !validates_input { return; }

    if norm.contains("response:") { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-route-missing-response-schema".into(),
        message: "Route validates input but has no `response:` schema — Eden/OpenAPI clients lose the success type.".into(),
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
    fn flags_post_with_body_no_response() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ a: t.String() }) });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_post_with_response_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ a: t.String() }), response: { 200: t.Object({ ok: t.Boolean() }) } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_route_without_validation() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/x', () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.post('/x', () => 'ok', { body: 1 });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
