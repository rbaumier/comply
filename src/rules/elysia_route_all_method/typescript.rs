//! elysia-route-all-method backend — flag `.all(` in Elysia route chains.

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
    if prop.utf8_text(source).unwrap_or("") != "all" {
        return;
    }

    // Require at least 2 args (path, handler) to look like a route registration.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    if !(args_text.starts_with("('") || args_text.starts_with("(\"") || args_text.starts_with("(`")) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-route-all-method".into(),
        message: "`.all()` matches any HTTP method — prefer a specific method (`.get`, `.post`, etc.) to communicate intent.".into(),
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
    fn flags_all_route() {
        let src = "import { Elysia } from 'elysia';\napp.all('/users', () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_chained_all() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().all('/health', handler);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_specific_method() {
        let src = "import { Elysia } from 'elysia';\napp.get('/users', () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.all('/users', () => 'ok');";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
