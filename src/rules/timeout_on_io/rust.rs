//! timeout-on-io backend for Rust.
//!
//! Flags bare `await` on known I/O calls (`reqwest::get`, `client.get`,
//! `sqlx::query`, etc.) without a `tokio::time::timeout` wrapper.
//! I/O without a timeout can hang the runtime indefinitely.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

/// Method-name suffixes that indicate I/O.
const IO_METHODS: &[&str] = &[
    "get", "post", "put", "delete", "patch", "request", "send", "execute",
    "query", "fetch_all", "fetch_one", "fetch_optional",
];

/// Callee bases that indicate I/O clients.
const IO_BASES: &[&str] = &["reqwest", "sqlx", "hyper", "http", "client"];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            // In tree-sitter-rust, `await` is a postfix unary: the AST node
            // kind is `await_expression` wrapping an inner expression.
            if node.kind() != "await_expression" {
                return;
            }
            let Some(inner) = node.named_child(0) else {
                return;
            };
            if !is_io_call(inner, source_bytes) {
                return;
            }
            if is_wrapped_in_timeout(node, source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "timeout-on-io".into(),
                message: "I/O call without a timeout — can hang the runtime \
                          forever on a slow peer. Wrap with \
                          `tokio::time::timeout(Duration::from_secs(5), ...)`."
                    .into(),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn is_io_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    let Ok(text) = function.utf8_text(source) else {
        return false;
    };
    // Match trailing method name + some base hint.
    for method in IO_METHODS {
        if text.ends_with(&format!(".{method}"))
            || text.ends_with(&format!("::{method}"))
        {
            // Require a known I/O base in the callee path.
            if IO_BASES.iter().any(|b| text.contains(b)) {
                return true;
            }
        }
    }
    false
}

/// True if the await is inside a `tokio::time::timeout(...)` wrapper.
fn is_wrapped_in_timeout(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "call_expression"
            && let Some(function) = parent.child_by_field_name("function")
            && let Ok(text) = function.utf8_text(source)
            && (text.contains("timeout") || text.contains("tokio::time"))
        {
            return true;
        }
        cur = parent;
    }
    false
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
    fn flags_bare_reqwest_get() {
        let source = "async fn f() { let r = reqwest::get(url).await; }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_timeout_wrapped_call() {
        let source = "async fn f() { tokio::time::timeout(d, reqwest::get(url)).await; }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_non_io_await() {
        let source = "async fn f() { let x = compute().await; }";
        assert!(run_on(source).is_empty());
    }
}
