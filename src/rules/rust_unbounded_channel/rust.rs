//! rust-unbounded-channel backend.
//!
//! Matches on the last `::`-separated segment of the called function's
//! path, so predicate methods like `range.is_unbounded()` are not
//! mistaken for channel constructors.
//!
//! Flags:
//! - `unbounded_channel` (tokio's `tokio::sync::mpsc::unbounded_channel`).
//! - `unbounded` (crossbeam's `crossbeam::channel::unbounded`).
//! - `channel` when the file uses `std::sync::mpsc` — `std::sync::mpsc`
//!   has no bounded `channel()` (the bounded one is `sync_channel(N)`),
//!   so a zero-arg `mpsc::channel()` is always unbounded. Tokio's
//!   `mpsc::channel(N)` takes a capacity, so calls with arguments are
//!   left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir};

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
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let Ok(text) = function.utf8_text(source_bytes) else {
            return;
        };
        // Match on the last `::`-separated path segment so a bare predicate
        // method like `range.is_unbounded()` (a `field_expression` whose text
        // happens to end in `unbounded`) is not mistaken for a constructor.
        let last_segment = text.rsplit("::").next().unwrap_or(text);
        let is_unbounded = last_segment == "unbounded_channel"
            || last_segment == "unbounded"
            || last_segment == "channel" && is_inside_mpsc_use(node, source_bytes);
        if !is_unbounded {
            return;
        }
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        // mpsc::channel — only flag if it's `std::sync::mpsc` (which is
        // always unbounded). Tokio's `mpsc::channel(N)` takes a capacity
        // and is the right call. We distinguish by argument count:
        // unbounded variants take zero args, tokio's bounded variant
        // takes one.
        if last_segment == "channel" {
            let arg_count = node
                .child_by_field_name("arguments")
                .map(|args| {
                    let mut cur = args.walk();
                    args.named_children(&mut cur).count()
                })
                .unwrap_or(0);
            if arg_count > 0 {
                return;
            }
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-unbounded-channel".into(),
            message: format!(
                "`{text}(...)` returns an unbounded queue — a slow \
                 consumer will OOM the process. Use `mpsc::channel(N)` \
                 or `crossbeam::channel::bounded(N)` to get backpressure."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Last-resort heuristic for the bare `channel()` call (no scoping).
/// True if the file has `use std::sync::mpsc` or `use mpsc::*` somewhere.
fn is_inside_mpsc_use(_node: tree_sitter::Node, source: &[u8]) -> bool {
    let text = std::str::from_utf8(source).unwrap_or("");
    text.contains("std::sync::mpsc") || text.contains("use mpsc")
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
    fn flags_tokio_unbounded_channel() {
        let source = "fn f() { let (tx, rx) = tokio::sync::mpsc::unbounded_channel(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_std_mpsc_channel() {
        let source = "use std::sync::mpsc;\nfn f() { let (tx, rx) = mpsc::channel(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_crossbeam_unbounded() {
        let source = "fn f() { let (tx, rx) = crossbeam::channel::unbounded(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_tokio_bounded_channel() {
        let source = "fn f() { let (tx, rx) = tokio::sync::mpsc::channel(1024); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_crossbeam_bounded() {
        let source = "fn f() { let (tx, rx) = crossbeam::channel::bounded(1024); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_std_sync_channel_with_capacity() {
        let source = "use std::sync::mpsc;\nfn f() { let (tx, rx) = mpsc::sync_channel(1024); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unbounded_channel_in_test_fn() {
        let source = "#[test]\nfn it_works() { let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<u8>(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unbounded_channel_in_tokio_test() {
        let source = "#[tokio::test]\nasync fn it_works() { let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<u8>(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unbounded_channel_in_tests_dir() {
        let source = "fn f() { let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<u8>(); }";
        assert!(crate::rules::test_helpers::run_rule(&Check, source, "tests/my_test.rs").is_empty());
    }

    #[test]
    fn allows_is_unbounded_predicate_method() {
        // Issue #3219: `range.is_unbounded()` is a predicate, not a constructor.
        let source =
            "fn f(num_vals: Option<ValueRange>) -> bool { num_vals.unwrap_or_default().is_unbounded() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_check_unbounded_predicate_method() {
        let source = "fn f(x: T) -> bool { x.check_unbounded() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unbounded_field_access_method() {
        let source = "fn f(range: Range) -> bool { range.is_unbounded() }";
        assert!(run_on(source).is_empty());
    }
}
