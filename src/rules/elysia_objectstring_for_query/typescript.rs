//! elysia-objectstring-for-query backend — query string fields can not carry
//! nested `t.Object(...)`; use `t.ObjectString({...})` for JSON-encoded objects.

use crate::diagnostic::{Diagnostic, Severity};

const STOP_KEYS: &[&str] = &[
    "body:",
    "params:",
    "headers:",
    "response:",
    "cookie:",
    "detail:",
    "tags:",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

    let Some(idx) = norm.find("query:t.Object({") else { return };
    let after_outer = &norm[idx + "query:t.Object({".len()..];

    let cut = STOP_KEYS
        .iter()
        .filter_map(|k| after_outer.find(k))
        .min()
        .unwrap_or(after_outer.len());
    let section = &after_outer[..cut];

    if !section.contains("t.Object(") { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-objectstring-for-query".into(),
        message: "Nested `t.Object(...)` in a `query:` schema cannot validate query strings — use `t.ObjectString({...})`.".into(),
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
    fn flags_nested_object_in_query() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 1, { query: t.Object({ filter: t.Object({ a: t.String() }) }) });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_object_string_in_query() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 1, { query: t.Object({ filter: t.ObjectString({ a: t.String() }) }) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_flat_query_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 1, { query: t.Object({ q: t.String() }) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/x', () => 1, { query: t.Object({ filter: t.Object({ a: 1 }) }) });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
