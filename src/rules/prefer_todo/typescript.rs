//! prefer-todo (TS/JS/TSX) — flag `test('x', () => {})` with an empty body.
//!
//! A `test` or `it` call whose callback has a zero-statement
//! `statement_block` is a placeholder that silently passes. Prefer
//! `test.todo('description')` so the runner reports it as pending and it
//! cannot be confused with a real test.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" { return; }
    let name = func.utf8_text(source).unwrap_or("");
    if name != "test" && name != "it" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(cb) = args.named_child(1) else { return };
    if !matches!(cb.kind(), "arrow_function" | "function_expression" | "function") {
        return;
    }

    let Some(body) = cb.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" { return; }
    if body.named_child_count() != 0 { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-todo".into(),
        message: format!(
            "Empty `{name}` body — use `{name}.todo('...')` to mark this as a \
             placeholder so the runner reports it as pending.",
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_empty_test_arrow() {
        assert_eq!(run_on("test('x', () => {});").len(), 1);
    }

    #[test]
    fn flags_empty_it_function() {
        assert_eq!(run_on("it('x', function () {});").len(), 1);
    }

    #[test]
    fn allows_test_todo() {
        assert!(run_on("test.todo('x');").is_empty());
    }

    #[test]
    fn allows_test_with_body() {
        assert!(run_on("test('x', () => { expect(1).toBe(1); });").is_empty());
    }

    #[test]
    fn ignores_non_test_calls() {
        assert!(run_on("foo('x', () => {});").is_empty());
    }
}
