//! rust-unbounded-channel backend.
//!
//! Flags two patterns:
//! - Any call whose function path ends in `unbounded_channel`
//!   (tokio's `tokio::sync::mpsc::unbounded_channel` etc.)
//! - `std::sync::mpsc::channel()` — note that `std::sync::mpsc`
//!   has no bounded variant of `channel()`; the bounded one is
//!   `std::sync::mpsc::sync_channel(N)`. So a bare `mpsc::channel()`
//!   call is always unbounded.
//!
//! `crossbeam::channel::unbounded()` is the same risk; we catch it
//! via the `unbounded` suffix match.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

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
        // Match by suffix on the dotted/scoped path.
        let is_unbounded = text.ends_with("unbounded_channel")
            || text.ends_with("unbounded")
            || text.ends_with("mpsc::channel")
            || text == "channel"
            && is_inside_mpsc_use(node, source_bytes);
        if !is_unbounded {
            return;
        }
        // mpsc::channel — only flag if it's `std::sync::mpsc` (which is
        // always unbounded). Tokio's `mpsc::channel(N)` takes a capacity
        // and is the right call. We distinguish by argument count:
        // unbounded variants take zero args, tokio's bounded variant
        // takes one.
        if text.ends_with("mpsc::channel") || text == "channel" {
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
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


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
}
