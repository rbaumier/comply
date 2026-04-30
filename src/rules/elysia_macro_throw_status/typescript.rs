//! elysia-macro-throw-status backend — flag `throw status(...)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["throw_statement"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    let norm: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if !norm.contains("throwstatus(") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-macro-throw-status".into(),
        message: "Use `return status(...)` instead of `throw status(...)` so Elysia tracks the response type.".into(),
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
    fn flags_throw_status() {
        let src =
            "import { Elysia, status } from 'elysia';\nfunction guard() { throw status(401); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_return_status() {
        let src =
            "import { Elysia, status } from 'elysia';\nfunction guard() { return status(401); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "function guard() { throw status(401); }";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
