//! elysia-numeric-body-no-coerce backend — `t.Number()` inside a body schema
//! does not auto-coerce; flag and recommend `t.Numeric()`.

use crate::diagnostic::{Diagnostic, Severity};

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

    // Find a `body:t.Object({...})` slice and check it contains `t.Number(`.
    let Some(body_idx) = norm.find("body:t.") else { return };
    let after_body = &norm[body_idx..];

    // Rough end of body section: the next top-level option key.
    let cut = ["params:", "query:", "headers:", "response:", "cookie:", "detail:", "tags:"]
        .iter()
        .filter_map(|k| after_body.find(k))
        .min()
        .unwrap_or(after_body.len());
    let body_section = &after_body[..cut];

    if !body_section.contains("t.Number(") { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-numeric-body-no-coerce".into(),
        message: "`t.Number()` in a `body:` schema rejects numeric strings — use `t.Numeric()` if the body can be form-encoded.".into(),
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
    fn flags_number_in_body() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ age: t.Number() }) });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_numeric_in_body() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ age: t.Numeric() }) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_number_in_response_only() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ name: t.String() }), response: { 200: t.Object({ count: t.Number() }) } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.post('/x', () => 'ok', { body: t.Object({ age: t.Number() }) });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
