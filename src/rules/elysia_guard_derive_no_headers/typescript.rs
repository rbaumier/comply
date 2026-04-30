//! elysia-guard-derive-no-headers backend — flag `.guard()` with header reads but no `headers:` schema.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["headers.auth", "headers.authorization"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "guard" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    // Need a header read.
    let reads_header = args_text.contains("headers.authorization") || args_text.contains("headers.auth");
    if !reads_header {
        return;
    }

    // First arg should be the config object.
    let Some(first) = args.named_child(0) else { return };
    if first.kind() != "object" {
        return;
    }
    let config_text = first.utf8_text(source).unwrap_or("");
    let norm: String = config_text.chars().filter(|c| !c.is_whitespace()).collect();
    if norm.contains("headers:") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-guard-derive-no-headers".into(),
        message: "Guard reads `headers.authorization` without a `headers:` schema — add one so the field is validated.".into(),
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
    fn flags_guard_with_header_read_and_no_schema() {
        let src = "import { Elysia } from 'elysia';\napp.guard({ beforeHandle: ({ headers }) => headers.authorization }, app => app);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_resolve_reading_header() {
        let src = "import { Elysia } from 'elysia';\napp.guard({ resolve: ({ headers }) => ({ token: headers.authorization }) }, app => app);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_guard_with_headers_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.guard({ headers: t.Object({ authorization: t.String() }), beforeHandle: ({ headers }) => headers.authorization }, app => app);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src =
            "app.guard({ beforeHandle: ({ headers }) => headers.authorization }, app => app);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
