//! elysia-static-await-hmr backend — flag `staticPlugin()` without `await`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "staticPlugin" {
        return;
    }

    // Check whether the parent of this call is an await_expression.
    if let Some(parent) = node.parent() {
        if parent.kind() == "await_expression" {
            return;
        }
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-static-await-hmr".into(),
        message: "`staticPlugin()` is async — use `await staticPlugin()` so HMR picks up file changes.".into(),
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
    fn flags_use_static_plugin_without_await() {
        let src = "import { Elysia } from 'elysia';\nimport { staticPlugin } from '@elysiajs/static';\nnew Elysia().use(staticPlugin());";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_use_static_plugin_with_await() {
        let src = "import { Elysia } from 'elysia';\nimport { staticPlugin } from '@elysiajs/static';\nasync function main() { return new Elysia().use(await staticPlugin()); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_files_without_static_import() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().use(staticPlugin());";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
