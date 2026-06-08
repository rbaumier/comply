//! timeout-on-io backend for Rust.
//!
//! Flags bare `await` on known I/O calls (`reqwest::get`, `client.get`,
//! `sqlx::query`, etc.) without a `tokio::time::timeout` wrapper.
//! I/O without a timeout can hang the runtime indefinitely.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

/// Method-name suffixes that indicate I/O.
const IO_METHODS: &[&str] = &[
    "get",
    "post",
    "put",
    "delete",
    "patch",
    "request",
    "send",
    "execute",
    "query",
    "fetch_all",
    "fetch_one",
    "fetch_optional",
];

/// Callee bases that indicate I/O clients.
const IO_BASES: &[&str] = &["reqwest", "sqlx", "hyper", "http"];

const KINDS: &[&str] = &["await_expression"];

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
        if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
            return;
        }
        if ctx.path.to_string_lossy().contains("/examples/") {
            return;
        }
        let source_bytes = ctx.source.as_bytes();
        if is_in_test_context(node, source_bytes) {
            return;
        }
        // In tree-sitter-rust, `await` is a postfix unary: the AST node
        // kind is `await_expression` wrapping an inner expression.
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
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "timeout-on-io".into(),
            message: "I/O call without a timeout — can hang the runtime \
                      forever on a slow peer. Wrap with \
                      `tokio::time::timeout(Duration::from_secs(5), ...)`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_test_path(path: &std::path::Path) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/");
    lower.starts_with("tests/")
        || lower.starts_with("test/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("/__tests__/")
        || lower.contains(".test.")
        || lower.contains(".spec.")
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
        if text.ends_with(&format!(".{method}")) || text.ends_with(&format!("::{method}")) {
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

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
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

    #[test]
    fn flags_bare_sqlx_query() {
        let source = "async fn f() { sqlx::query(\"SELECT *\").execute(&pool).await; }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_generic_client_get() {
        let source = "async fn f() { let res = client.get(\"/\").await; }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_io_await_in_test_files() {
        let source = "async fn f() { let r = reqwest::get(url).await; }";
        assert!(run_on_path(source, "tests/client.rs").is_empty());
    }

    #[test]
    fn allows_timeout_with_duration() {
        let source = "async fn f() { tokio::time::timeout(Duration::from_secs(5), client.get(url).send()).await; }";
        assert!(run_on(source).is_empty());
    }
}
