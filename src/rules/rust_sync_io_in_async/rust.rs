//! rust-sync-io-in-async backend.
//!
//! Walks `call_expression` nodes whose function path matches a known
//! sync I/O API (the standard library `std::fs::*` filesystem helpers
//! and `std::net::TcpStream::*` networking helpers) and verifies the
//! call sits inside an `async fn`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_inside_async_fn;

const KINDS: &[&str] = &["call_expression"];

/// Function suffixes that indicate a blocking std::fs / std::net call.
/// We match by `ends_with` so any qualified path (`std::fs::read_to_string`,
/// `fs::read_to_string`, etc.) is caught equally.
const SYNC_IO_SUFFIXES: &[&str] = &[
    // std::fs
    "std::fs::read",
    "std::fs::read_to_string",
    "std::fs::write",
    "std::fs::create_dir",
    "std::fs::create_dir_all",
    "std::fs::remove_file",
    "std::fs::remove_dir",
    "std::fs::remove_dir_all",
    "std::fs::rename",
    "std::fs::copy",
    "std::fs::metadata",
    // std::net
    "std::net::TcpStream::connect",
    "std::net::TcpStream::connect_timeout",
    "std::net::TcpListener::bind",
    "std::net::TcpListener::accept",
    "std::net::UdpSocket::bind",
];

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
        let Some(matched) = matched_sync_api(text) else {
            return;
        };
        if !is_inside_async_fn(node, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-sync-io-in-async".into(),
            message: format!(
                "`{matched}(..)` is a blocking syscall inside an `async fn` — \
                 it parks the worker thread for the whole I/O. Use the \
                 `tokio::fs`/`tokio::net` equivalent, or wrap in \
                 `tokio::task::spawn_blocking`."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Match the call's function path against the sync-IO list.
///
/// We only flag fully-qualified `std::*` paths. Shortened forms like
/// `fs::read_to_string` are ambiguous — they could be `std::fs` (bad)
/// or `tokio::fs` (fine), and we don't carry import scope. Erring on
/// the side of false negatives keeps the rule trustworthy.
fn matched_sync_api(text: &str) -> Option<&'static str> {
    SYNC_IO_SUFFIXES.iter().copied().find(|full| text == *full)
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_std_fs_read_in_async() {
        let source = r#"async fn f() { let _ = std::fs::read_to_string("a.txt"); }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_std_fs_write_in_async() {
        let source = r#"async fn f() { std::fs::write("a.txt", "x"); }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_std_fs_in_sync_fn() {
        let source = r#"fn f() { let _ = std::fs::read_to_string("a.txt"); }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_tokio_fs_in_async_fn() {
        let source = r#"async fn f() { let _ = tokio::fs::read_to_string("a.txt").await; }"#;
        assert!(run_on(source).is_empty());
    }
}
