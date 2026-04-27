//! elysia-derive-async-no-await — flag `.derive(async ...)` whose body
//! contains no `await`.

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
    if prop.utf8_text(source).unwrap_or("") != "derive" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let text = args.utf8_text(source).unwrap_or("");
    if !text.contains("async") {
        return;
    }
    if text.contains("await ") || text.contains("await(") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-derive-async-no-await".into(),
        message: "`.derive(async ...)` body never awaits — handlers receive a Promise and must explicitly await it. Drop `async` or add an `await`.".into(),
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
    fn flags_async_derive_no_await() {
        let src = "import { Elysia } from 'elysia';\napp.derive(async () => ({ id: 1 }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_async_derive_with_await() {
        let src = "import { Elysia } from 'elysia';\napp.derive(async () => ({ user: await getUser() }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_sync_derive() {
        let src = "import { Elysia } from 'elysia';\napp.derive(() => ({ id: 1 }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.derive(async () => ({ id: 1 }));";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
