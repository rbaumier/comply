//! rust-thread-sleep-in-async backend.
//!
//! Walks `call_expression` nodes whose function path ends in
//! `thread::sleep` or is a bare `sleep`/`sleep_ms` (when paired with
//! a sync std::thread import). Then verifies the call is inside an
//! `async fn` via the shared `is_inside_async_fn` helper.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_inside_async_fn, is_inside_spawned_closure};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(text) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !is_thread_sleep_call(text, ctx.source) {
        return;
    }
    if !is_inside_async_fn(node, source) {
        return;
    }
    if is_inside_spawned_closure(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "rust-thread-sleep-in-async".into(),
        message: format!(
            "`{text}(..)` blocks the OS thread — inside an `async fn` this \
             freezes the runtime worker. Use `tokio::time::sleep(d).await` \
             instead."
        ),
        severity: Severity::Error,
        span: None,
    });
}

fn is_thread_sleep_call(text: &str, source: &str) -> bool {
    // Qualified calls — always flag.
    if text.ends_with("thread::sleep") || text.ends_with("thread::sleep_ms") {
        return true;
    }
    // Bare `sleep`/`sleep_ms` — only flag when it comes from std::thread.
    if text == "sleep" || text == "sleep_ms" {
        return has_std_thread_import(source) && !has_async_sleep_import(source);
    }
    false
}

/// Returns `true` when the file imports `std::thread` (module or specific fn).
fn has_std_thread_import(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("use std::thread") || trimmed.starts_with("use ::std::thread")
    })
}

/// Returns `true` when the file imports `sleep` from an async runtime.
fn has_async_sleep_import(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("use tokio::time::sleep")
            || (trimmed.starts_with("use tokio::time::{") && trimmed.contains("sleep"))
            || trimmed.starts_with("use async_std::task::sleep")
            || (trimmed.starts_with("use async_std::task::{") && trimmed.contains("sleep"))
    })
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
    fn flags_thread_sleep_in_async_fn() {
        let source = "async fn f() { std::thread::sleep(std::time::Duration::from_secs(1)); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_thread_sleep_in_sync_fn() {
        let source = "fn f() { std::thread::sleep(std::time::Duration::from_secs(1)); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_tokio_sleep_in_async_fn() {
        let source = "async fn f() { tokio::time::sleep(d).await; }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_tokio_sleep_import_in_async_fn() {
        let source = "use tokio::time::sleep;\nasync fn f() { sleep(d).await; }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_async_std_sleep_import_in_async_fn() {
        let source = "use async_std::task::sleep;\nasync fn f() { sleep(d).await; }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_bare_sleep_with_std_thread_import() {
        let source = "use std::thread::sleep;\nasync fn f() { sleep(d); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_bare_sleep_with_std_thread_module_import() {
        let source = "use std::thread;\nasync fn f() { thread::sleep(d); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_bare_sleep_without_any_import() {
        // Unknown origin — don't flag to avoid false positives.
        let source = "async fn f() { sleep(d); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_thread_sleep_in_thread_spawn_closure() {
        let source = r#"
            #[tokio::test]
            async fn test() {
                thread::spawn(|| {
                    thread::sleep(std::time::Duration::from_millis(100));
                });
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_thread_sleep_in_spawn_blocking_closure() {
        let source = r#"
            async fn test() {
                tokio::task::spawn_blocking(|| {
                    thread::sleep(std::time::Duration::from_millis(100));
                });
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_thread_sleep_in_builder_spawn_closure() {
        let source = r#"
            async fn test() {
                std::thread::Builder::new()
                    .spawn(|| {
                        thread::sleep(std::time::Duration::from_millis(100));
                    });
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_direct_sleep_in_async() {
        let source = r#"
            async fn test() {
                thread::sleep(std::time::Duration::from_millis(100));
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }
}
