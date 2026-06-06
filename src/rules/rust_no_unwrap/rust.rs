//! rust-no-unwrap backend.
//!
//! Flags `.unwrap()` and `.expect(...)` method calls in non-test code.
//! These turn runtime conditions (None / Err) into panics, which is the
//! opposite of what production code should do. Prefer `?` + proper error
//! types, or `unwrap_or_else` with a meaningful fallback.
//!
//! Tests are exempted — `.unwrap()` in a unit test is idiomatic because
//! a panic cleanly fails the test. We skip any call whose enclosing
//! function has `#[test]` or whose enclosing module has `#[cfg(test)]`.
//!
//! build.rs is exempted — panics in Cargo build scripts are an acceptable
//! error mode during compilation (e.g. env::var("FOO").unwrap()).
//!
//! Lock operations are exempted — `.read().unwrap()`, `.write().unwrap()`,
//! `.lock().unwrap()` are idiomatic for std::sync::{Mutex,RwLock} poisoning.
//!
//! This rule is equivalent to `clippy::unwrap_used` + `clippy::expect_used`
//! (both restriction-group lints, off by default in clippy). Running it
//! via comply means you get the check without having to enable the lints
//! in every consuming crate.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

const KINDS: &[&str] = &["call_expression"];

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
        if ctx.path.file_name() == Some(std::ffi::OsStr::new("build.rs")) {
            return;
        }
        let source_bytes = ctx.source.as_bytes();
        // Looking for `receiver.unwrap()` / `receiver.expect("…")`.
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "field_expression" {
            return;
        }
        let Some(field) = function.child_by_field_name("field") else {
            return;
        };
        let Ok(field_text) = field.utf8_text(source_bytes) else {
            return;
        };
        if field_text != "unwrap" && field_text != "expect" {
            return;
        }
        // Skip test code — `.unwrap()` is fine there.
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        // Skip lock operations — .read()/.write()/.lock()/.try_lock() unwrap is idiomatic.
        if field_text == "unwrap" {
            let receiver = function.child_by_field_name("value");
            if let Some(recv) = receiver {
                if recv.kind() == "call_expression" {
                    if let Some(inner_func) = recv.child_by_field_name("function") {
                        if inner_func.kind() == "field_expression" {
                            if let Some(inner_field) = inner_func.child_by_field_name("field") {
                                if let Ok(method) = inner_field.utf8_text(source_bytes) {
                                    if matches!(method, "read" | "write" | "lock" | "try_lock" | "try_read" | "try_write") {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-unwrap".into(),
            message: format!(
                "`.{field_text}()` turns a runtime condition into a panic. \
                 Use `?` with a proper error type, or `unwrap_or_else` with \
                 a meaningful fallback. Tests are exempted."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True if `path` lives under Cargo's `tests/` integration-test
/// directory. Cargo compiles every `tests/*.rs` (and any modules
/// they import via `tests/common/mod.rs`) as `cfg(test)`, so the
/// rule treats them as test code without requiring an explicit
/// `#[cfg(test)]` annotation.
fn is_under_tests_dir(path: &std::path::Path) -> bool {
    path.components().any(|c| c.as_os_str() == "tests")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_unwrap_in_production_fn() {
        assert_eq!(run_on("fn f() { let x = y.unwrap(); }").len(), 1);
    }

    #[test]
    fn flags_expect_in_production_fn() {
        assert_eq!(run_on(r#"fn f() { let x = y.expect("msg"); }"#).len(), 1);
    }

    #[test]
    fn allows_unwrap_in_test_function() {
        let source = "#[test]\nfn it_works() { let x = y.unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_inside_cfg_test_module() {
        let source = "#[cfg(test)]\nmod tests { fn f() { let x = y.unwrap(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_question_mark() {
        assert!(run_on("fn f() -> Result<(), ()> { let x = y?; Ok(()) }").is_empty());
    }

    #[test]
    fn allows_unwrap_in_build_rs() {
        let source = r#"fn main() { let v = std::env::var("TARGET").unwrap(); }"#;
        assert!(crate::rules::test_helpers::run_rust_with_path(source, &Check, "build.rs").is_empty());
    }

    #[test]
    fn allows_unwrap_on_rwlock_read() {
        let source = "fn f(data: &RwLock<Vec<u8>>) -> Vec<u8> { data.read().unwrap().clone() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_on_mutex_lock() {
        let source = "fn f(m: &Mutex<u32>) -> u32 { *m.lock().unwrap() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_non_lock_unwrap() {
        assert_eq!(run_on("fn f() { let x = y.unwrap(); }").len(), 1);
    }
}
