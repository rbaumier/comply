//! elysia-cookie-removal-api backend — flag `cookie.x.value = ''` patterns.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["assignment_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let left_text = left.utf8_text(source).unwrap_or("");
    if !left_text.starts_with("cookie.") || !left_text.ends_with(".value") {
        return;
    }

    let Some(right) = node.child_by_field_name("right") else { return };
    let right_text = right.utf8_text(source).unwrap_or("").trim();
    let is_empty_string = right_text == "''" || right_text == "\"\"" || right_text == "``";
    let is_null = right_text == "null" || right_text == "undefined";
    if !is_empty_string && !is_null {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cookie-removal-api".into(),
        message: format!("`{left_text} = {right_text}` does not clear the cookie — call `cookie.<name>.remove()` instead."),
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
    fn flags_empty_string_assignment() {
        let src = "import { Elysia } from 'elysia';\napp.get('/logout', ({ cookie }) => { cookie.session.value = ''; });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_null_assignment() {
        let src = "import { Elysia } from 'elysia';\napp.get('/logout', ({ cookie }) => { cookie.session.value = null; });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_remove_call() {
        let src = "import { Elysia } from 'elysia';\napp.get('/logout', ({ cookie }) => { cookie.session.remove(); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "cookie.session.value = '';";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
