//! rust-thread-spawn-in-async backend.
//!
//! Walks `call_expression` nodes whose function path ends in
//! `thread::spawn` and verifies the call is inside an `async fn`.
//! Mirrors `rust-block-on-in-async` but for the thread::spawn
//! footgun.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::call_expression::call_function_name;
use crate::rules::rust_helpers::{is_inside_async_fn, is_in_test_context, is_under_tests_dir};

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
        let source_bytes = ctx.source.as_bytes();
        let Some(name) = call_function_name(node, source_bytes) else {
            return;
        };
        if !name.ends_with("thread::spawn") {
            return;
        }
        if !is_inside_async_fn(node, source_bytes) {
            return;
        }
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-thread-spawn-in-async",
            format!(
                "`{name}(..)` from inside an `async fn` defeats the runtime. \
                 Use `tokio::spawn` for futures, or \
                 `tokio::task::spawn_blocking` for sync CPU work."
            ),
            Severity::Warning,
        ));
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
    fn flags_thread_spawn_in_async() {
        let source = "async fn f() { std::thread::spawn(|| {}); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_bare_thread_spawn_in_async() {
        let source = "async fn f() { thread::spawn(|| {}); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_thread_spawn_in_sync_fn() {
        let source = "fn f() { std::thread::spawn(|| {}); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_tokio_spawn_in_async() {
        let source = "async fn f() { tokio::spawn(other()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_spawn_blocking_in_async() {
        let source = "async fn f() { tokio::task::spawn_blocking(|| {}); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_thread_spawn_in_test_fn() {
        let source = "#[test]\nfn f() { thread::spawn(|| {}); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_thread_spawn_in_tokio_test() {
        let source = "#[tokio::test]\nasync fn f() { std::thread::spawn(|| {}); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_thread_spawn_in_raw_identifier_async_fn() {
        // `fn r#async` is a sync fn named with a raw identifier, not an
        // `async fn` — `std::thread::spawn` inside it is legitimate.
        let source = "impl S { fn r#async(s: T) -> S { \
                      let h = std::thread::spawn(move || work(s)); S(h) } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_thread_spawn_in_genuine_async_fn() {
        let source = "async fn run() { std::thread::spawn(|| work()); }";
        assert_eq!(run_on(source).len(), 1);
    }
}
