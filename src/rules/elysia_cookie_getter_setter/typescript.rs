//! elysia-cookie-getter-setter backend — flag `cookie.get(` / `cookie.set(`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "cookie.get" && callee_text != "cookie.set" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cookie-getter-setter".into(),
        message: "Use `cookie.<name>.value` instead of `cookie.get/set(...)` — Elysia cookies are reactive accessors.".into(),
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
    fn flags_cookie_get() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/', ({ cookie }) => cookie.get('session'));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_cookie_set() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/', ({ cookie }) => cookie.set('session', 'x'));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_cookie_value_access() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/', ({ cookie }) => cookie.session.value);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "cookie.get('session');";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
