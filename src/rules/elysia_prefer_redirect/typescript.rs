//! elysia-prefer-redirect backend — flag manual redirect patterns.

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

    let Some(right) = node.child_by_field_name("right") else { return };
    let right_text = right.utf8_text(source).unwrap_or("").trim();
    if right_text != "301" && right_text != "302" && right_text != "303" && right_text != "307" && right_text != "308" {
        return;
    }

    // Confirm the file actually sets a Location header somewhere.
    let s = ctx.source;
    let has_location = s.contains("set.headers.location")
        || s.contains("set.headers['location']")
        || s.contains("set.headers[\"location\"]")
        || s.contains("set.headers.Location")
        || s.contains("set.headers['Location']")
        || s.contains("set.headers[\"Location\"]");
    if !has_location {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-prefer-redirect".into(),
        message: "Manual redirect via `set.status` + `set.headers.location` — return `redirect(url, code)` instead.".into(),
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
    fn flags_manual_302_redirect() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', ({ set }) => { set.status = 302; set.headers.location = '/new'; });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_manual_301_redirect() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', ({ set }) => { set.status = 301; set.headers['Location'] = '/new'; });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_redirect_helper() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', ({ redirect }) => redirect('/new', 302));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_redirect_status() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', ({ set }) => { set.status = 401; });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
