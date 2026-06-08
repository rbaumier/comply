//! rust-no-sleep-in-test backend.
//!
//! Mirror image of `rust-thread-sleep-in-async`: that rule fires on
//! sleep calls in `async fn` bodies; this rule fires on sleep calls
//! in test code (any context where `is_in_test_context` returns
//! true, plus files under Cargo's `tests/` integration-test
//! directory).
//!
//! Wall-clock sleep in tests is the canonical source of slow + flaky
//! suites: the timing is wrong on CI under load, and the sleep
//! always pays its full cost on the happy path. Replace with a
//! condition wait (channel, polled deadline) or with tokio's
//! virtual-time helpers (`tokio::time::pause` + `advance`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::call_expression::call_function_name;
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
        let source_bytes = ctx.source.as_bytes();
        let Some(name) = call_function_name(node, source_bytes) else {
            return;
        };
        if !is_sleep_call(name) {
            return;
        }
        if !is_in_test_context(node, source_bytes) && !is_under_tests_dir(ctx.path) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-sleep-in-test".into(),
            message: format!(
                "`{name}(..)` in test code makes the suite slow and flaky. \
                 Wait on a condition (channel, polled deadline), or use \
                 `tokio::time::pause()` + `advance(d)` for virtual time."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_sleep_call(name: &str) -> bool {
    // Sync `std::thread::sleep` / `std::thread::sleep_ms`.
    name.ends_with("thread::sleep")
        || name.ends_with("thread::sleep_ms")
        // Async `tokio::time::sleep` / `tokio::time::sleep_until`.
        || name.ends_with("tokio::time::sleep")
        || name.ends_with("tokio::time::sleep_until")
        || name.ends_with("time::sleep")
        || name.ends_with("time::sleep_until")
        // Bare `sleep` after `use std::thread::sleep` / `use tokio::time::sleep`.
        || name == "sleep"
        || name == "sleep_ms"
        || name == "sleep_until"
}

/// True if `path` lives under Cargo's `tests/` integration-test
/// directory, which Cargo always compiles as `cfg(test)`.
fn is_under_tests_dir(path: &std::path::Path) -> bool {
    path.components().any(|c| c.as_os_str() == "tests")
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
    fn flags_thread_sleep_in_test_fn() {
        let source =
            "#[test]\nfn slow() { std::thread::sleep(std::time::Duration::from_secs(1)); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_sleep_in_cfg_test_module() {
        let source = "#[cfg(test)]\nmod tests { fn helper() { \
                      std::thread::sleep(std::time::Duration::from_secs(1)); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_tokio_sleep_in_cfg_test_module() {
        let source = "#[cfg(test)]\nmod tests { async fn helper() { \
                      tokio::time::sleep(d).await; } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_sleep_in_production_fn() {
        let source = "fn f() { std::thread::sleep(std::time::Duration::from_secs(1)); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unrelated_call_in_test() {
        let source = "#[test]\nfn it_works() { do_thing(); }";
        assert!(run_on(source).is_empty());
    }
}
