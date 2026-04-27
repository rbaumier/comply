//! elysia-service-return-not-throw backend — flag `throw` in elysia service files.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["throw_statement"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let _ = source;
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-service-return-not-throw".into(),
        message: "`throw` in Elysia code breaks typed error propagation — return `status(code, message)` instead.".into(),
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
    fn flags_throw_new_error() {
        let src = "import { Elysia } from 'elysia';\nfunction svc() { throw new Error('boom'); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_throw_string() {
        let src = "import { Elysia } from 'elysia';\nfunction svc() { throw 'no'; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_status_return() {
        let src = "import { Elysia, status } from 'elysia';\nfunction svc() { return status(404, 'not found'); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "function svc() { throw new Error('boom'); }";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
