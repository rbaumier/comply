//! rust-no-sleep-in-test backend.
//!
//! Mirror image of `rust-thread-sleep-in-async`: that rule fires on
//! sleep calls in `async fn` bodies; this rule fires on sleep calls
//! in inline test code (any context where `is_in_test_context`
//! returns true).
//!
//! Wall-clock sleep in tests is the canonical source of slow + flaky
//! suites: the timing is wrong on CI under load, and the sleep
//! always pays its full cost on the happy path. Replace with a
//! condition wait (channel, polled deadline) or with tokio's
//! virtual-time helpers (`tokio::time::pause` + `advance`).
//!
//! Three legitimate sleep patterns are exempt:
//! - Files under Cargo's `tests/` integration-test directory:
//!   integration tests are black-box clients of real systems, where
//!   a wall-clock wait on external readiness (e.g. a remote consumer
//!   connecting) can be unavoidable. The rule's value is in unit
//!   tests, where you control both sides of the sync point.
//! - Tests annotated `#[tokio::test(start_paused = true)]`: the
//!   Tokio clock is paused, so `time::sleep` advances simulated time
//!   instantly instead of blocking — never slow or flaky.
//! - Sleeps inside a bounded retry loop (a loop containing a
//!   `break`): polling a condition with an early exit is the correct
//!   way to wait when no sync primitive exists, not a flaky fixed
//!   wait.

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
        if is_under_tests_dir(ctx.path) {
            return;
        }
        if !is_in_test_context(node, source_bytes) {
            return;
        }
        if is_in_start_paused_tokio_test(node, source_bytes) {
            return;
        }
        if is_in_bounded_retry_loop(node) {
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
/// directory. Such files are exempt: integration tests against
/// external services often have no introspectable sync point, so a
/// wall-clock wait is the only available readiness signal.
fn is_under_tests_dir(path: &std::path::Path) -> bool {
    path.components().any(|c| c.as_os_str() == "tests")
}

/// True if the test function enclosing `node` carries
/// `#[tokio::test(start_paused = true)]`. Under a paused clock, Tokio's
/// `time::sleep` advances simulated time instantly instead of blocking the
/// thread, so it is a yield point — not a flaky wall-clock wait.
fn is_in_start_paused_tokio_test(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            return function_has_start_paused_attr(parent, source);
        }
        cur = parent;
    }
    false
}

/// True if `item` has a preceding `attribute_item` sibling whose text, with
/// whitespace removed, contains `start_paused=true` (e.g.
/// `#[tokio::test(start_paused = true)]`). `start_paused` is a tokio::test
/// parameter, so this match is specific to paused tokio tests. A
/// `start_paused = false` attribute does NOT match.
fn function_has_start_paused_attr(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source) {
            let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            if compact.contains("start_paused=true") {
                return true;
            }
        }
        sibling = s.prev_named_sibling();
    }
    false
}

const LOOP_KINDS: &[&str] = &["for_expression", "while_expression", "loop_expression"];
const SCOPE_BOUNDARY_KINDS: &[&str] = &["function_item", "closure_expression"];

/// True if the sleep call sits inside a loop (within the enclosing
/// function or closure) that contains a `break`. A loop with a
/// conditional `break` is bounded condition polling — the sleep
/// throttles the retries and exits as soon as the condition holds —
/// not a fixed flaky wait.
fn is_in_bounded_retry_loop(node: tree_sitter::Node) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if SCOPE_BOUNDARY_KINDS.contains(&parent.kind()) {
            return false;
        }
        if LOOP_KINDS.contains(&parent.kind()) && subtree_contains_break(parent) {
            return true;
        }
        current = parent;
    }
    false
}

/// True if `node`'s subtree contains a `break` expression, without
/// descending into nested functions or closures.
fn subtree_contains_break(node: tree_sitter::Node) -> bool {
    if node.kind() == "break_expression" {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| !SCOPE_BOUNDARY_KINDS.contains(&child.kind()))
        .any(subtree_contains_break)
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

    #[test]
    fn allows_sleep_in_integration_test_dir() {
        // svix tests/it/sqs_consumer.rs: waiting for an external SQS
        // consumer to connect has no introspectable sync point.
        let source = "#[tokio::test]\nasync fn t() { tokio::time::sleep(d).await; }";
        let diagnostics =
            crate::rules::test_helpers::run_rule(&Check, source, "tests/it/sqs_consumer.rs");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn allows_sleep_in_bounded_retry_loop() {
        // tantivy assert_eventually: bounded polling with an early break.
        let source = "#[cfg(test)]\nmod tests { fn assert_eventually() { \
                      for _ in 0..30 { if check() { break; } \
                      std::thread::sleep(d); } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_sleep_in_loop_without_break() {
        let source = "#[test]\nfn slow() { for _ in 0..5 { std::thread::sleep(d); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_sleep_in_start_paused_tokio_test_issue_1023() {
        let source =
            "#[tokio::test(start_paused = true)]\nasync fn t() { tokio::time::sleep(d).await; }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_sleep_in_closure_inside_start_paused_tokio_test() {
        // tokio's own unit tests: the sleep sits inside a stream
        // combinator closure, still governed by the paused clock.
        let source = "#[tokio::test(start_paused = true)]\nasync fn t() { \
                      let s = stream::iter([5]).then(move |n| \
                      time::sleep(d).map(move |_| n)); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_sleep_in_start_paused_tokio_test_without_spaces() {
        let source =
            "#[tokio::test(start_paused=true)]\nasync fn t() { tokio::time::sleep(d).await; }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_sleep_in_plain_tokio_test() {
        let source = "#[tokio::test]\nasync fn t() { tokio::time::sleep(d).await; }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_sleep_when_start_paused_is_false() {
        let source =
            "#[tokio::test(start_paused = false)]\nasync fn t() { tokio::time::sleep(d).await; }";
        assert_eq!(run_on(source).len(), 1);
    }
}
