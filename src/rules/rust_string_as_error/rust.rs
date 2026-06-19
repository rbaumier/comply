//! rust-string-as-error backend.
//!
//! Walks every `generic_type` and flags `Result<_, String>` wherever the error
//! type is an unforced local choice — free-function return types, inherent-impl
//! methods, struct fields, type aliases. Suppressed in trait method signatures
//! (trait definitions and trait impls), where the error type is a fixed public
//! API contract the author can't change without breaking callers.

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
    // A `Result<_, String>` in a trait method signature is a public API contract:
    // the trait fixes the error type and every impl must conform — neither can
    // switch to a structured error unilaterally. Flag String-as-error only where it
    // is an unforced local choice (free/inherent functions, struct fields, aliases).
    if crate::rules::rust_helpers::is_in_trait_impl(node)
        || crate::rules::rust_helpers::is_in_trait_definition(node)
    {
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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

    #[test]
    fn allows_result_string_in_trait_definition() {
        // The trait fixes the error type as part of its public API contract.
        let src = "pub trait T { fn f(&self) -> Result<i32, String>; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_string_in_trait_impl() {
        // A conforming impl can't change the contract unilaterally.
        let src = "impl T for S { fn f(&self) -> Result<i32, String> { Ok(0) } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_result_string_in_inherent_impl() {
        // No trait contract — the author chose `String` freely.
        let src = "impl S { fn f(&self) -> Result<i32, String> { Ok(0) } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_result_string_in_struct_field() {
        // A struct field is not a trait method signature.
        assert_eq!(run_on("struct S { e: Result<i32, String> }").len(), 1);
    }
}
