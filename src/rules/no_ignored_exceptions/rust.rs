//! no-ignored-exceptions Rust backend — flag `let _ = fallible()` that
//! discards a Result/Option without handling it.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "let_declaration" {
        return;
    }

    // Check if the pattern is `_` (wildcard).
    let Some(pattern) = node.child_by_field_name("pattern") else { return };
    let Ok(pat_text) = pattern.utf8_text(source) else { return };
    if pat_text != "_" {
        return;
    }

    // Must have a value (right-hand side).
    let Some(value) = node.child_by_field_name("value") else { return };

    // The value should be a call expression or method call (likely fallible).
    let is_call = matches!(
        value.kind(),
        "call_expression" | "macro_invocation" | "await_expression"
            | "try_expression" | "field_expression"
    );
    // Also check for method calls: `obj.method()` parses as call_expression
    // with a field_expression callee.
    if !is_call {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-ignored-exceptions".into(),
        message: "`let _ = ...` discards a potentially fallible result \u{2014} handle the error or use `drop()`.".into(),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_let_underscore_call() {
        let src = "fn f() { let _ = do_something(); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_let_underscore_macro() {
        let src = "fn f() { let _ = try_parse!(input); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_named_binding() {
        let src = "fn f() { let _result = do_something(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_underscore_literal() {
        let src = "fn f() { let _ = 42; }";
        assert!(run_on(src).is_empty());
    }
}
