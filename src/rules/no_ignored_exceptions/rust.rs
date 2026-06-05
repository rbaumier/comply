//! no-ignored-exceptions Rust backend — flag `let _ = fallible()` that
//! discards a Result/Option without handling it.
//!
//! Tests are exempted: a `let _ = fn_under_test()` pattern is the
//! idiomatic way to assert "this call doesn't panic" without caring
//! about the return value. Skipped via
//! `rust_helpers::is_in_test_context`.
//!
//! NOTE: This rule uses a heuristic (call-like pattern matching) rather than
//! type awareness. It may flag `let _ = infallible_fn()` where the function
//! provably does not return Result/Option. Without --type-aware, there is no
//! fix for this class of FP — document intent in the calling code if needed.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;

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
}
