//! rust-string-as-error backend.
//!
//! Walks every `generic_type` and flags `Result<_, String>` patterns.
//! Same approach as `rust-unit-error-result`: AST-only, no scope
//! analysis, so it catches the type wherever it appears (function
//! return types, struct fields, type aliases, etc.).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::result_error_type;

crate::ast_check! { on ["generic_type"] => |node, source, ctx, diagnostics|
    let Some(err_type) = result_error_type(node, source) else {
        return;
    };
    let Ok(err_text) = err_type.utf8_text(source) else {
        return;
    };
    if err_text.trim() != "String" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "rust-string-as-error".into(),
        message: "`Result<_, String>` is stringly-typed — callers can't \
                  pattern-match failure modes. Define a proper error enum \
                  (use `thiserror::Error`)."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_result_string_error() {
        assert_eq!(run_on("fn f() -> Result<i32, String> { Ok(0) }").len(), 1);
    }

    #[test]
    fn allows_result_with_real_error_type() {
        assert!(run_on("fn f() -> Result<i32, MyError> { Ok(0) }").is_empty());
    }

    #[test]
    fn allows_result_unit_error() {
        // Unit-error is a different rule (`rust-unit-error-result`).
        // This rule only flags String — keep concerns separate.
        assert!(run_on("fn f() -> Result<i32, ()> { Ok(0) }").is_empty());
    }
}
