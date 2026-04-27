//! elysia-transform-no-schema backend — flag transform({ body }) without body schema.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = match callee.kind() {
        "member_expression" => callee
            .child_by_field_name("property")
            .map(|p| p.utf8_text(source).unwrap_or(""))
            .unwrap_or(""),
        "identifier" => callee.utf8_text(source).unwrap_or(""),
        _ => "",
    };
    if callee_text != "transform" && callee_text != "onTransform" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    if !args_text.contains("body") {
        return;
    }
    // File contains a body schema declaration somewhere (`body: t.` is the standard form).
    if ctx.source.contains("body: t.") || ctx.source.contains("body:t.") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-transform-no-schema".into(),
        message: "`transform` accesses `body` but no `body:` schema is declared — declare one so the body is validated before mutation.".into(),
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
    fn flags_transform_without_schema() {
        let src = "import { Elysia } from 'elysia';\napp.transform(({ body }) => { body.email = body.email.toLowerCase(); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_on_transform_without_schema() {
        let src = "import { Elysia } from 'elysia';\napp.onTransform(({ body }) => { body.normalized = true; });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_transform_with_body_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.post('/u', handler, { body: t.Object({ email: t.String() }) });\napp.transform(({ body }) => { body.email = body.email.toLowerCase(); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.transform(({ body }) => body);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
