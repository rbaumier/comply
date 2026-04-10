//! rust-unit-error-result backend.
//!
//! Walks every type expression and flags `Result<_, ()>` patterns.
//! We match on the AST, not on text, so it catches the type wherever
//! it appears: function return types, struct fields, type aliases,
//! generic bounds, etc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::result_error_type;

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some(err_type) = result_error_type(node, source) else {
        return;
    };
    if err_type.kind() != "unit_type" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "rust-unit-error-result".into(),
        message: "`Result<_, ()>` discards every error detail. Define a \
                  real error type, or return `Option<T>` if absence is the \
                  only failure mode."
            .into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_result_unit_error_in_return() {
        assert_eq!(run_on("fn f() -> Result<i32, ()> { Ok(0) }").len(), 1);
    }

    #[test]
    fn flags_result_unit_error_in_field() {
        assert_eq!(
            run_on("struct S { last: Result<u8, ()> }").len(),
            1
        );
    }

    #[test]
    fn allows_result_with_real_error() {
        assert!(run_on("fn f() -> Result<i32, String> { Ok(0) }").is_empty());
    }

    #[test]
    fn allows_io_result_alias() {
        // `io::Result<T>` only takes one type arg — we can't see the
        // error from the AST so we don't flag it.
        assert!(run_on("fn f() -> io::Result<()> { Ok(()) }").is_empty());
    }
}
