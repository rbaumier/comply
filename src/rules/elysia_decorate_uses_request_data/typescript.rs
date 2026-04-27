//! elysia-decorate-uses-request-data — flag `.decorate(...)` calls whose
//! argument list mentions `Date.now()` or `Math.random()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "decorate" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let text = args.utf8_text(source).unwrap_or("");
    if !text.contains("Date.now()") && !text.contains("Math.random()") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-decorate-uses-request-data".into(),
        message: "`.decorate(...)` runs once at boot — `Date.now()` / `Math.random()` here freezes a single value for every request. Use `.derive(...)` for per-request data.".into(),
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
    fn flags_decorate_with_date_now() {
        let src = "import { Elysia } from 'elysia';\napp.decorate('startedAt', Date.now());";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_decorate_with_math_random() {
        let src = "import { Elysia } from 'elysia';\napp.decorate('id', Math.random());";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_decorate_with_static_value() {
        let src = "import { Elysia } from 'elysia';\napp.decorate('config', { url: 'x' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.decorate('id', Math.random());";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
