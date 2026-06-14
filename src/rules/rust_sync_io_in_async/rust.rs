//! rust-sync-io-in-async backend.
//!
//! Walks `call_expression` nodes whose function path matches a known
//! sync I/O API (the standard library `std::fs::*` filesystem helpers
//! and `std::net::TcpStream::*` networking helpers) and verifies the
//! call sits inside an `async fn`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::call_expression::call_function_name;
use crate::rules::rust_helpers::{has_test_attribute, is_inside_async_fn};

const KINDS: &[&str] = &["call_expression"];

/// Final path segments of the blocking-offload combinators that run a
/// closure off the async runtime worker. `spawn_blocking` and
/// `block_in_place` are tokio public API; `asyncify` is tokio's
/// documented `spawn_blocking(f).await` wrapper used throughout
/// `tokio::fs`. A blocking syscall inside a closure handed to one of
/// these never parks the executor, so it is not a finding.
const OFFLOAD_COMBINATORS: &[&str] = &["spawn_blocking", "block_in_place", "asyncify"];

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
        if is_inside_offload_closure(node, source_bytes) {
            return;
        }
        if is_inside_test_fn(node, source_bytes) {
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

/// True if `node` sits inside a `closure_expression` that is itself an
/// argument to a call whose callee's final path segment is one of
/// `OFFLOAD_COMBINATORS`. Such a closure runs on the blocking thread
/// pool, so the sync syscall does not park the async worker.
///
/// Walks the ancestor chain, stopping at the enclosing `function_item`
/// so a closure in an outer scope can't exempt a call in the async fn
/// body. Closures handed to non-offload combinators (`map`, `filter`,
/// …) are not exempted.
fn is_inside_offload_closure(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            return false;
        }
        if parent.kind() == "closure_expression"
            && let Some(args) = parent.parent()
            && args.kind() == "arguments"
            && let Some(call) = args.parent()
            && call.kind() == "call_expression"
            && let Some(fn_text) = call_function_name(call, source)
            && is_offload_combinator(fn_text)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if `text` (a call's `function` text, e.g. `tokio::task::spawn_blocking`,
/// `spawn_blocking`, or `asyncify`) has a final path segment in
/// `OFFLOAD_COMBINATORS`.
fn is_offload_combinator(text: &str) -> bool {
    let segment = text.rsplit("::").next().unwrap_or(text);
    OFFLOAD_COMBINATORS.contains(&segment)
}

/// True if `node`'s nearest enclosing `function_item` carries a test
/// attribute (`#[test]`, `#[tokio::test]`, `#[async_std::test]`,
/// `#[actix_web::test]`, and their `(...)`-argument forms). Sync I/O in a
/// test body blocks only that test's dedicated runtime, not a production
/// worker pool, so it is not a finding.
fn is_inside_test_fn(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            return has_test_attribute(parent, source);
        }
        cur = parent;
    }
    false
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

    #[test]
    fn allows_std_fs_inside_asyncify_closure() {
        let source = r#"async fn f() { asyncify(|| std::fs::copy(a, b)).await; }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_std_fs_inside_spawn_blocking_closure() {
        let source = r#"async fn f() { spawn_blocking(move || std::fs::write(p, c)).await; }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_std_fs_inside_block_in_place_closure() {
        let source = r#"async fn f() { tokio::task::block_in_place(|| std::fs::read(p)); }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_std_fs_directly_in_async_body() {
        let source = r#"async fn f() { std::fs::read(p); }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_std_fs_in_non_offload_closure() {
        let source = r#"async fn f() { items.iter().map(|x| std::fs::read(x)); }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_std_fs_in_tokio_test_multi_thread() {
        let source = r#"
            #[tokio::test(flavor = "multi_thread")]
            async fn t() { let _ = std::fs::read_to_string(p); }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_std_fs_in_tokio_test() {
        let source = r#"
            #[tokio::test]
            async fn t() { std::fs::create_dir(p); }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_std_fs_in_bare_test_fn() {
        let source = r#"
            #[test]
            fn t() { let _ = std::fs::read(p); }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_std_fs_in_non_test_async_fn() {
        let source = r#"async fn handler() { let _ = std::fs::read(p); }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_std_fs_in_tokio_main() {
        let source = r#"
            #[tokio::main]
            async fn main() { let _ = std::fs::read(p); }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }
}
