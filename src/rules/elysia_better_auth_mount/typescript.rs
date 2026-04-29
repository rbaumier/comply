//! elysia-better-auth-mount backend — flag `.use(auth.handler)` for Better Auth.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["auth.handler"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".use") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    if !norm.contains("auth.handler") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-better-auth-mount".into(),
        message: "Use `.mount(auth.handler)` instead of `.use(auth.handler)` — Better Auth requires the WHATWG mount path.".into(),
        severity: Severity::Error,
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
    fn flags_use_auth_handler() {
        let src = "import { auth } from './auth';\nimport 'better-auth';\napp.use(auth.handler);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_mount_auth_handler() {
        let src = "import { auth } from './auth';\nimport 'better-auth';\napp.mount(auth.handler);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_better_auth_files() {
        let src = "app.use(auth.handler);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
