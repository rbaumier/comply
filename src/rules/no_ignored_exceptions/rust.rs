//! no-ignored-exceptions Rust backend — flag `let _ = fallible()` that
//! discards a Result/Option without handling it.
//!
//! Tests are exempted: a `let _ = fn_under_test()` pattern is the
//! idiomatic way to assert "this call doesn't panic" without caring
//! about the return value. Skipped via
//! `rust_helpers::is_in_test_context`.
//!
//! Two non-error idioms are also exempted:
//! - `let _ = Arc::from_raw(p)` / `Box::from_raw(p)` (and bare `from_raw`):
//!   reconstructing an owning pointer from a raw pointer and dropping it to
//!   run its `Drop` impl. The reconstruction is infallible — `let _ =` invokes
//!   the destructor, it does not ignore an error.
//! - compile-fail test fixtures under a `tests/.../fail/` directory: `let _ =`
//!   suppresses "unused result" warnings so they don't pollute the expected
//!   compiler error output of `trybuild`/`tests-build` cases.
//!
//! NOTE: This rule uses a heuristic (call-like pattern matching) rather than
//! type awareness. It may flag `let _ = infallible_fn()` where the function
//! provably does not return Result/Option. Without --type-aware, there is no
//! fix for this class of FP — document intent in the calling code if needed.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;
use tree_sitter::Node;

crate::ast_check! { on ["let_declaration"] => |node, source, ctx, diagnostics|
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
    if !is_call {
        return;
    }

    // Skip inside tests — `let _ = …` there is "call and don't care".
    if is_in_test_context(node, source) {
        return;
    }

    // Skip compile-fail test fixtures (`tests/.../fail/`): `let _ =` there
    // suppresses "unused result" warnings in the expected compiler output.
    if is_compile_fail_fixture(ctx.path) {
        return;
    }

    // Skip the intentional-drop idiom `let _ = Arc/Box::from_raw(p)`: the
    // reconstruction is infallible and exists only to run the value's `Drop`.
    if is_from_raw_reconstruction(value, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-ignored-exceptions".into(),
        message: "`let _ = ...` discards a potentially fallible result \u{2014} handle the error or use `drop()`.".into(),
        severity: Severity::Error,
        span: None,
    });
}

/// True for compile-fail fixtures: a `fail` directory component nested under a
/// `tests` component (`tests/fail/`, `tests-build/tests/fail/`,
/// `*_compile_tests/tests/fail/`). Both components must be present so ordinary
/// `fail/` directories outside a test harness are still checked.
fn is_compile_fail_fixture(path: &std::path::Path) -> bool {
    let mut seen_tests = false;
    for component in path.components() {
        let segment = component.as_os_str();
        if segment == "tests" {
            seen_tests = true;
        } else if segment == "fail" && seen_tests {
            return true;
        }
    }
    false
}

/// True if `value` is `Arc::from_raw(..)` / `Box::from_raw(..)` /
/// `Rc::from_raw(..)` or a bare `from_raw(..)` call — the reconstruct-and-drop
/// idiom used in `RawWakerVTable::drop` and similar destructors.
fn is_from_raw_reconstruction(value: Node, source: &[u8]) -> bool {
    if value.kind() != "call_expression" {
        return false;
    }
    let Some(function) = value.child_by_field_name("function") else {
        return false;
    };
    let Ok(callee) = function.utf8_text(source) else {
        return false;
    };
    let name = callee.rsplit("::").next().unwrap_or(callee);
    name == "from_raw"
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

    #[test]
    fn allows_let_underscore_call_inside_test_function() {
        // The user's reported FP family — a `#[test]` fn where
        // `let _ = …` asserts "no panic" without consuming the value.
        let src = r#"
            #[test]
            fn missing_config_falls_back_to_defaults() {
                let cfg = Config::load_from(tmp.path()).unwrap();
                let _ = cfg.threshold("max-function-lines", "max");
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_underscore_call_inside_cfg_test_module() {
        let src = r#"
            #[cfg(test)]
            mod tests {
                fn helper() {
                    let _ = do_something();
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_underscore_call_inside_tokio_test() {
        let src = r#"
            #[tokio::test]
            async fn test_send_side_effect() {
                let _ = tx.send(item).await;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_underscore_call_inside_actix_test() {
        let src = r#"
            #[actix_rt::test]
            async fn test_cleanup() {
                let _ = handle.abort();
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_underscore_from_raw_intentional_drop() {
        // Regression for #1408: reconstructing an owning pointer to run its
        // Drop is infallible, not an ignored error.
        let arc = "unsafe fn drop_waker(raw: *const ()) { let _ = Arc::from_raw(raw); }";
        let boxed = "unsafe fn drop_box(raw: *const ()) { let _ = Box::from_raw(raw); }";
        let bare = "unsafe fn drop_waker(raw: *const ()) { let _ = from_raw(raw); }";
        assert!(run_on(arc).is_empty());
        assert!(run_on(boxed).is_empty());
        assert!(run_on(bare).is_empty());
    }

    #[test]
    fn allows_let_underscore_in_compile_fail_fixture() {
        // Regression for #1408: compile-fail fixtures use `let _ =` to keep
        // "unused result" warnings out of the expected compiler output.
        let src = "fn f() { let _ = tokio::try_join!(async {}); }";
        let diagnostics = crate::rules::test_helpers::run_rule(
            &Check,
            src,
            "tests-build/tests/fail/macros_try_join.rs",
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn flags_let_underscore_in_ordinary_fail_dir() {
        // The exemption requires a `tests` ancestor; a plain `fail/` dir
        // outside a test harness is still a genuinely ignored result.
        let src = "fn f() { let _ = do_something(); }";
        let diagnostics =
            crate::rules::test_helpers::run_rule(&Check, src, "src/fail/handler.rs");
        assert_eq!(diagnostics.len(), 1);
    }
}
