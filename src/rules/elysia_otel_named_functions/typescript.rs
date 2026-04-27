//! elysia-otel-named-functions backend — flag arrow functions in `.derive`/`.resolve` under opentelemetry.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    let last = callee_text.rsplit('.').next().unwrap_or("");
    if last != "derive" && last != "resolve" {
        return;
    }
    if !callee_text.contains('.') {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let named: Vec<_> = args.named_children(&mut cursor).collect();
    // The handler is the last argument (first arg may be a scope string/options).
    let Some(handler) = named.last() else { return };
    if handler.kind() != "arrow_function" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-otel-named-functions".into(),
        message: "Arrow function in `.derive`/`.resolve` — OpenTelemetry spans will be unnamed; use a named function.".into(),
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
    fn flags_arrow_in_derive() {
        let src = "import { opentelemetry } from '@elysiajs/opentelemetry';\napp.derive(async ({ headers }) => ({ user: null }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_named_function_in_derive() {
        let src = "import { opentelemetry } from '@elysiajs/opentelemetry';\napp.derive(async function deriveUser({ headers }) { return { user: null }; });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_otel_files() {
        let src = "app.derive(async ({ headers }) => ({ user: null }));";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
