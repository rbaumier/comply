//! rust-thread-sleep-in-async backend.
//!
//! Walks `call_expression` nodes whose function path ends in
//! `thread::sleep` or is a bare `sleep`/`sleep_ms` (when paired with
//! a sync std::thread import). Then verifies the call is inside an
//! `async fn` via the shared `is_inside_async_fn` helper.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_inside_async_fn;
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "call_expression" {
                return;
            }
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            let Ok(text) = function.utf8_text(source_bytes) else {
                return;
            };
            if !is_thread_sleep_call(text) {
                return;
            }
            if !is_inside_async_fn(node, source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-thread-sleep-in-async".into(),
                message: format!(
                    "`{text}(..)` blocks the OS thread — inside an `async fn` \
                     this freezes the runtime worker. Use \
                     `tokio::time::sleep(d).await` instead."
                ),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

fn is_thread_sleep_call(text: &str) -> bool {
    text.ends_with("thread::sleep")
        || text.ends_with("thread::sleep_ms")
        || text == "sleep"
        || text == "sleep_ms"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.rs"), source),
            &tree,
        )
    }

    #[test]
    fn flags_thread_sleep_in_async_fn() {
        let source =
            "async fn f() { std::thread::sleep(std::time::Duration::from_secs(1)); }";
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
}
