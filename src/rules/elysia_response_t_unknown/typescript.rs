//! elysia-response-t-unknown — inside an object literal that contains a
//! `response:` property, flag the property when its value is `t.Unknown()`
//! or `t.Any()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).unwrap_or("");
    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    if key_name != "response" {
        return;
    }
    let Some(value) = node.child_by_field_name("value") else { return };
    let val_text = value.utf8_text(source).unwrap_or("").trim();
    if !(val_text.starts_with("t.Unknown(") || val_text.starts_with("t.Any(")) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-response-t-unknown".into(),
        message: "`response: t.Unknown()` / `t.Any()` disables response validation — describe the shape with a concrete TypeBox schema.".into(),
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
    fn flags_response_t_unknown() {
        let src = "import { Elysia, t } from 'elysia';\napp.get('/x', () => 1, { response: t.Unknown() });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_response_t_any() {
        let src =
            "import { Elysia, t } from 'elysia';\napp.get('/x', () => 1, { response: t.Any() });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_response_concrete_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.get('/x', () => 1, { response: t.Object({ id: t.String() }) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const x = { response: t.Unknown() };";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
