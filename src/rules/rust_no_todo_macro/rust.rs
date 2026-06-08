//! rust-no-todo-macro backend.
//!
//! Flags `todo!()` invocations outside of test code. `todo!` is a
//! placeholder marker that aborts at runtime — production binaries
//! must not ship with one. Tests are exempted because panicking
//! inside `#[test]` is a clean failure mode.
//!
//! `unimplemented!` / `panic!` / `unreachable!` are handled by the
//! broader `rust-no-panic-macros` rule. This rule is the targeted
//! `todo!`-only variant for users who want to ban the placeholder
//! independently.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir};

const KINDS: &[&str] = &["macro_invocation"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(macro_name_node) = node.child_by_field_name("macro") else {
            return;
        };
        let Ok(macro_name) = macro_name_node.utf8_text(source_bytes) else {
            return;
        };
        if macro_name != "todo" {
            return;
        }
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-todo-macro".into(),
            message: "`todo!()` is a placeholder that panics at runtime. \
                      Implement the path, or return a typed `Result` error \
                      for the unsupported case. Tests are exempted."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
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
    fn flags_todo_in_production_fn() {
        assert_eq!(run_on("fn f() { todo!(); }").len(), 1);
    }

    #[test]
    fn flags_todo_with_message() {
        assert_eq!(run_on(r#"fn f() { todo!("wire this up"); }"#).len(), 1);
    }

    #[test]
    fn allows_todo_in_test_fn() {
        let source = "#[test]\nfn it_works() { todo!(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_todo_in_cfg_test_module() {
        let source = "#[cfg(test)]\nmod tests { fn helper() { todo!(); } }";
        assert!(run_on(source).is_empty());
    }

    fn run_with_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn does_not_flag_panic() {
        assert!(run_on(r#"fn f() { panic!("boom"); }"#).is_empty());
    }

    #[test]
    fn does_not_flag_unimplemented() {
        assert!(run_on("fn f() { unimplemented!(); }").is_empty());
    }

    #[test]
    fn allows_todo_in_integration_test_file() {
        let source = "impl IntoResponse for Stub { fn into_response(self) { todo!(); } }";
        assert!(run_with_path(source, "axum-macros/tests/debug_handler/fail/wrong_return_tuple.rs").is_empty());
    }

    #[test]
    fn allows_todo_in_tests_subdirectory() {
        let source = "fn stub() { todo!(); }";
        assert!(run_with_path(source, "crate/tests/fixtures/stub.rs").is_empty());
    }
}
