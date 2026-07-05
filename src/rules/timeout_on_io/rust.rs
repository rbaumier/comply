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
        if is_wrapped_in_timeout(node, source_bytes)
            || is_reqwest_timeout_protected(inner, source_bytes)
        {
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

/// True when a reqwest I/O call is bounded by reqwest's own `.timeout(...)`,
/// which makes a `tokio::time::timeout` wrapper unnecessary. Two signals:
/// - request-level: the call's own chain carries `.timeout(...)`
///   (`RequestBuilder::timeout`);
/// - client-level: the base receiver's `let` binding is built by a builder
///   chain containing `.timeout(...)` (`ClientBuilder::timeout`), which bounds
///   every request that client issues.
fn is_reqwest_timeout_protected(io_call: tree_sitter::Node, source: &[u8]) -> bool {
    let (request_has_timeout, base) = walk_receiver_spine(io_call, source);
    if request_has_timeout {
        return true;
    }
    let Some(base) = base else {
        return false;
    };
    // Only a plain local binding (`let client = ...`) can be resolved; a
    // path like `reqwest::get` or a `self.client` receiver cannot.
    if base.kind() != "identifier" {
        return false;
    }
    let Ok(name) = base.utf8_text(source) else {
        return false;
    };
    find_let_initializer(io_call, name, source)
        .is_some_and(|init| walk_receiver_spine(init, source).0)
}

/// Walk the receiver spine of a method-call chain from `io_call` to its base
/// receiver. Returns `(spine_has_timeout, base)`: `spine_has_timeout` is true
/// when any `.timeout(...)` method call appears in the chain, `base` is the
/// leftmost receiver node (an `identifier` for a local binding).
fn walk_receiver_spine<'a>(
    io_call: tree_sitter::Node<'a>,
    source: &[u8],
) -> (bool, Option<tree_sitter::Node<'a>>) {
    let mut node = io_call;
    let mut has_timeout = false;
    loop {
        match node.kind() {
            "call_expression" => {
                let Some(func) = node.child_by_field_name("function") else {
                    return (has_timeout, None);
                };
                if is_timeout_field(func, source) {
                    has_timeout = true;
                }
                node = func;
            }
            "field_expression" => {
                let Some(value) = node.child_by_field_name("value") else {
                    return (has_timeout, None);
                };
                node = value;
            }
            "await_expression" | "try_expression" | "parenthesized_expression" => {
                let Some(child) = node.named_child(0) else {
                    return (has_timeout, None);
                };
                node = child;
            }
            "identifier" | "scoped_identifier" => return (has_timeout, Some(node)),
            _ => return (has_timeout, None),
        }
    }
}

/// True if `func` is the `function` of a `.timeout(...)` method call, i.e. a
/// `field_expression` whose field name is `timeout`.
fn is_timeout_field(func: tree_sitter::Node, source: &[u8]) -> bool {
    func.kind() == "field_expression"
        && func
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(source).ok())
            == Some("timeout")
}

/// Initializer of the nearest in-scope `let <name> = ...` binding preceding
/// `use_node` within its enclosing function body.
fn find_let_initializer<'a>(
    use_node: tree_sitter::Node<'a>,
    name: &str,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut cur = use_node;
    let body = loop {
        let parent = cur.parent()?;
        if parent.kind() == "function_item" {
            break parent.child_by_field_name("body")?;
        }
        cur = parent;
    };
    let mut best: Option<tree_sitter::Node<'a>> = None;
    find_binding(body, name, use_node.start_byte(), source, &mut best);
    best.and_then(|decl| decl.child_by_field_name("value"))
}

/// Record the latest `let <name> = ...` declaration under `node` that starts
/// before `use_start` into `best`.
fn find_binding<'a>(
    node: tree_sitter::Node<'a>,
    name: &str,
    use_start: usize,
    source: &[u8],
    best: &mut Option<tree_sitter::Node<'a>>,
) {
    if node.kind() == "let_declaration"
        && node.start_byte() < use_start
        && node
            .child_by_field_name("pattern")
            .filter(|p| p.kind() == "identifier")
            .and_then(|p| p.utf8_text(source).ok())
            == Some(name)
        && best.is_none_or(|b| b.start_byte() < node.start_byte())
    {
        *best = Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_binding(child, name, use_start, source, best);
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

    #[test]
    fn allows_reqwest_client_builder_timeout() {
        let source = "async fn f(registry: &str) -> anyhow::Result<()> { \
            let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build()?; \
            let _m = client.get(format!(\"https://x/{registry}\")).send().await?.json().await?; \
            Ok(()) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_reqwest_request_builder_timeout() {
        let source = "async fn g(url: &str) -> anyhow::Result<()> { \
            let _r = reqwest::Client::new().get(url).timeout(std::time::Duration::from_secs(3)).send().await?; \
            Ok(()) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_reqwest_client_without_timeout() {
        let source = "async fn h(url: &str) -> anyhow::Result<()> { \
            let client = reqwest::Client::new(); \
            let _r = client.get(\"http://x\").send().await?; \
            Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }
}
