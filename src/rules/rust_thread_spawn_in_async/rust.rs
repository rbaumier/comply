//! rust-thread-spawn-in-async backend.
//!
//! Flags `call_expression` nodes whose function path ends in
//! `thread::spawn` when the call is inside an `async fn` — spawning an OS
//! thread from async work usually means a future should have been
//! `tokio::spawn`ed instead. A `thread::spawn` whose closure hosts its own
//! Tokio runtime (drives `block_on` or constructs a runtime) is exempt: it
//! owns a dedicated runtime on its own OS thread and cannot be replaced by
//! `tokio::spawn`/`spawn_blocking`. Mirrors `rust-block-on-in-async`, which
//! recognises the inverse.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::call_expression::call_function_name;
use crate::rules::rust_helpers::{
    is_in_test_context, is_inside_async_fn, is_under_tests_dir, spawn_closure_hosts_runtime,
};

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
        if spawn_closure_hosts_runtime(node, source_bytes) {
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

    #[test]
    fn allows_thread_spawn_hosting_runtime_via_block_on() {
        // Dedicated-runtime-on-its-own-OS-thread: the closure builds a
        // separate runtime (via a helper) and drives it with `block_on`, so
        // `std::thread::spawn` is required and must not be flagged.
        let source = "async fn main() { std::thread::spawn(move || { \
                      let rt = make_rt(); rt.block_on(async move { work().await; }); }); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_thread_spawn_hosting_inline_runtime() {
        let source = "async fn f() { thread::spawn(|| { \
                      let rt = tokio::runtime::Runtime::new().unwrap(); \
                      rt.block_on(async {}); }); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_thread_spawn_building_runtime_without_lexical_block_on() {
        // Constructing a dedicated runtime is enough to mark the thread as a
        // runtime host, even with no `block_on` lexically in view.
        let source = "async fn f() { std::thread::spawn(|| { \
                      let rt = tokio::runtime::Runtime::new().unwrap(); run(rt); }); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_thread_spawn_building_runtime_via_builder() {
        // Builder form of runtime construction — the only exempting signal here
        // (no lexical `block_on`, no `Runtime::new`).
        let source = "async fn f() { std::thread::spawn(|| { \
                      let rt = tokio::runtime::Builder::new_current_thread()\
                      .enable_all().build().unwrap(); run(rt); }); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_thread_spawn_when_only_a_nested_inner_thread_hosts_a_runtime() {
        // The outer thread does stray sync work; a nested inner thread hosts
        // the runtime. The inner host must not exempt the outer stray spawn.
        let source = "async fn f() { std::thread::spawn(|| { do_sync_cpu_work(); \
                      std::thread::spawn(|| { \
                      let rt = tokio::runtime::Runtime::new().unwrap(); \
                      rt.block_on(async {}); }); }); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_stray_fire_and_forget_thread_in_async() {
        // No runtime hosting — a plain background thread doing sync CPU work
        // from an async fn is the footgun the rule targets.
        let source = "async fn f() { std::thread::spawn(|| { do_sync_cpu_work(); }); }";
        assert_eq!(run_on(source).len(), 1);
    }
}
