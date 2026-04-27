//! elysia-prefer-status-over-set backend — flag `set.status = N` assignments.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["assignment_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    if left.kind() != "member_expression" {
        return;
    }

    let Some(object) = left.child_by_field_name("object") else { return };
    let Some(property) = left.child_by_field_name("property") else { return };
    if object.utf8_text(source).unwrap_or("") != "set" {
        return;
    }
    if property.utf8_text(source).unwrap_or("") != "status" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-prefer-status-over-set".into(),
        message: "`set.status = code` is untyped — use `status(code, body)` for type-safe responses.".into(),
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
    fn flags_set_status_assignment() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', ({ set }) => { set.status = 401; return 'no'; });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_numeric_status() {
        let src = "import { Elysia } from 'elysia';\nfunction h(set) { set.status = 500; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_status_helper() {
        let src = "import { Elysia, status } from 'elysia';\napp.get('/', () => status(401, 'no'));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "function h(set) { set.status = 401; }";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
