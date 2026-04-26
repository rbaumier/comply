//! timeout-on-io backend — bare `await fetch(...)` / `await db.query(...)`
//! without an AbortSignal or withTimeout wrapper.
//!
//! Why: every I/O call without a timeout is a hang waiting to happen.
//! Network partitions, stuck connections, slow DNS — all of them turn
//! a normal-looking `await fetch(url)` into an infinite hang that
//! eventually exhausts the process's resources.
//!
//! Detection: walk `await_expression` nodes whose inner call expression
//! targets a known I/O callee (`fetch`, `db.query`, `axios.get/post/...`,
//! `http.get`, etc.) and flag those that don't pass an `AbortSignal` or
//! aren't wrapped in a `withTimeout(...)` helper.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const IO_CALLEE_BASES: &[&str] = &["fetch", "axios", "http", "https", "db"];
const IO_METHOD_SUFFIXES: &[&str] = &[
    "query",
    "execute",
    "get",
    "post",
    "put",
    "delete",
    "patch",
    "request",
    "send",
];

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
        let source_bytes = ctx.source.as_bytes();
        let Some(call) = inner_call(node) else {
            return;
        };
        if !is_io_call(call, source_bytes) {
            return;
        }
        if has_abort_signal_or_timeout(call, source_bytes) || is_wrapped_in_timeout(node, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "timeout-on-io".into(),
            message: "I/O call without a timeout — network calls can \
                      hang forever. Wrap with `withTimeout(..., 5_000)` \
                      or pass `{ signal: AbortSignal.timeout(5_000) }`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// If the await wraps a call_expression, return it.
fn inner_call(await_node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = await_node.walk();
    await_node
        .children(&mut cursor)
        .find(|child| child.kind() == "call_expression")
}

/// True if the call's function text matches a known I/O callee pattern.
fn is_io_call(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    let Ok(text) = function.utf8_text(source) else {
        return false;
    };
    // Bare identifier: `fetch(...)`.
    if IO_CALLEE_BASES.contains(&text) {
        return true;
    }
    // Dotted member: `foo.query`, `db.get`, `axios.post`.
    if let Some((base, method)) = text.rsplit_once('.') {
        if IO_CALLEE_BASES
            .iter()
            .any(|b| base == *b || base.ends_with(&format!(".{b}")))
            && IO_METHOD_SUFFIXES.contains(&method)
        {
            return true;
        }
        if IO_METHOD_SUFFIXES.contains(&method) && base.to_ascii_lowercase().contains("db") {
            return true;
        }
    }
    false
}

/// Look at the call's arguments for an AbortSignal / timeout option.
fn has_abort_signal_or_timeout(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let Ok(text) = args.utf8_text(source) else {
        return false;
    };
    text.contains("AbortSignal") || text.contains("signal:") || text.contains("timeout:")
}

/// True if the await is the inner call inside a `withTimeout(...)` wrapper.
fn is_wrapped_in_timeout(await_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = await_node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "call_expression"
            && let Some(fname) = parent.child_by_field_name("function")
            && fname
                .utf8_text(source)
                .is_ok_and(|t| t.contains("withTimeout") || t.contains("raceTimeout"))
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
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_bare_fetch() {
        assert_eq!(run_on("async function f() { await fetch(url); }").len(), 1);
    }

    #[test]
    fn allows_fetch_with_abort_signal() {
        let source = "async function f() { await fetch(url, { signal: AbortSignal.timeout(5000) }); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_with_timeout_wrapper() {
        let source = "async function f() { await withTimeout(fetch(url), 5000); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_bare_db_query() {
        assert_eq!(run_on("async function f() { await db.query('SELECT *'); }").len(), 1);
    }

    #[test]
    fn allows_non_io_await() {
        assert!(run_on("async function f() { await Promise.resolve(1); }").is_empty());
    }
}
