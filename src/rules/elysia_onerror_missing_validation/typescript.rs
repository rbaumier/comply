//! elysia-onerror-missing-validation backend — flag onError handlers that don't handle VALIDATION.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(property) = callee.child_by_field_name("property") else { return };
    if property.utf8_text(source).unwrap_or("") != "onError" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    if args_text.contains("VALIDATION") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-onerror-missing-validation".into(),
        message: "`onError` handler doesn't branch on `'VALIDATION'` — schema errors will surface as generic 500s.".into(),
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
    fn flags_onerror_without_validation() {
        let src = "import { Elysia } from 'elysia';\napp.onError(({ error }) => 'oops');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_onerror_with_other_codes() {
        let src = "import { Elysia } from 'elysia';\napp.onError(({ code }) => code === 'NOT_FOUND' ? 'nf' : 'oops');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_onerror_with_validation_branch() {
        let src = "import { Elysia } from 'elysia';\napp.onError(({ code, error }) => code === 'VALIDATION' ? error.message : 'oops');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.onError(() => 'oops');";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
