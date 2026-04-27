//! elysia-route-missing-auth backend — flag sensitive routes without auth handlers.

use crate::diagnostic::{Diagnostic, Severity};

const SENSITIVE: &[&str] = &["/admin", "/profile", "/me", "/settings", "/user"];
const HTTP_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "all"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if !HTTP_METHODS.contains(&method) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let has_sensitive_path = SENSITIVE.iter().any(|p| {
        args_text.contains(&format!("'{}", p))
            || args_text.contains(&format!("\"{}", p))
            || args_text.contains(&format!("`{}", p))
    });
    if !has_sensitive_path {
        return;
    }

    if args_text.contains("beforeHandle")
        || args_text.contains("auth")
        || args_text.contains("guard")
    {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-route-missing-auth".into(),
        message: "Sensitive route appears to have no auth guard — add `beforeHandle` or wrap it in `.guard({ auth: ... })`.".into(),
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
    fn flags_unguarded_admin_route() {
        let src = "import { Elysia } from 'elysia';\napp.get('/admin', () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_unguarded_me_route() {
        let src = "import { Elysia } from 'elysia';\napp.post('/me/avatar', handler);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_route_with_before_handle() {
        let src = "import { Elysia } from 'elysia';\napp.get('/admin', () => 'ok', { beforeHandle: requireAuth });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_public_route() {
        let src = "import { Elysia } from 'elysia';\napp.get('/health', () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/admin', () => 'ok');";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
