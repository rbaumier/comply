//! no-floating-promise backend — flag statement-level calls that look
//! like they return a Promise without being awaited or handled.
//!
//! Heuristic (no type info, so we're conservative):
//!   - The statement is `expression_statement` whose expression is a
//!     `call_expression` (or `await_expression` — we accept that, skip).
//!   - The call ends with `.then(...)`/`.catch(...)`/`.finally(...)` →
//!     already handled, skip.
//!   - The callee is `Promise.resolve/reject/all/allSettled/race/any` →
//!     flag (top-level promise literal).
//!   - The callee is a member whose method name is in the shared
//!     async-looking list (`save`, `fetch`, `query`, `connect`,
//!     `dispatch`, …; see `super::shared::ASYNC_LOOKING_METHODS`) — flag.
//!   - Otherwise skip.
//!
//! `delete` is intentionally omitted from the heuristic: `Map`, `Set`,
//! `WeakMap`, `WeakSet` `.delete(...)` all return `boolean`, and no
//! idiomatic JS/TS API exposes a Promise-returning `.delete(...)`.
//!
//! This intentionally misses cases (the spec mentions "skip if too
//! complex") rather than producing noisy false positives. Type-aware
//! rules catch the rest.

use crate::diagnostic::{Diagnostic, Severity};

use super::shared::ASYNC_LOOKING_METHODS;

// NOTE: the production oxc backend also flags a bare-identifier callee that
// resolves to a locally-declared `async function`/async arrow (a type-grounded
// signal). This tree-sitter backend has no semantic scope resolution, so that
// signal cannot be mirrored here; the two backends diverge on that case only.

/// Does the call end with `.then(...)` / `.catch(...)` / `.finally(...)`?
fn has_promise_handler(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    matches!(
        prop.utf8_text(source).unwrap_or(""),
        "then" | "catch" | "finally"
    )
}

/// Is the callee `Promise.<combinator>`?
fn is_promise_combinator(call: tree_sitter::Node, source: &[u8]) -> Option<&'static str> {
    let callee = call.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let obj = callee.child_by_field_name("object")?;
    let prop = callee.child_by_field_name("property")?;
    if obj.utf8_text(source).unwrap_or("") != "Promise" {
        return None;
    }
    match prop.utf8_text(source).unwrap_or("") {
        "resolve" => Some("resolve"),
        "reject" => Some("reject"),
        "all" => Some("all"),
        "allSettled" => Some("allSettled"),
        "race" => Some("race"),
        "any" => Some("any"),
        _ => None,
    }
}

/// Is the callee a member whose method name is in the async-looking list?
fn is_async_looking_member_call(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    let method = prop.utf8_text(source).unwrap_or("");
    ASYNC_LOOKING_METHODS.contains(&method)
}

crate::ast_check! { on ["expression_statement"] => |node, source, ctx, diagnostics|
    let Some(expr) = node.named_child(0) else { return };
    if expr.kind() != "call_expression" {
        return;
    }
    // Already handled by a promise handler — skip.
    if has_promise_handler(expr, source) {
        return;
    }
    let is_flag = is_promise_combinator(expr, source).is_some()
        || is_async_looking_member_call(expr, source);
    if !is_flag {
        return;
    }
    let pos = expr.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-floating-promise".into(),
        message: "Promise-returning call is used as a statement — rejections will \
                  become UnhandledPromiseRejection. Add `await`, chain `.catch`, \
                  or prefix with `void` if you really want to ignore it."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bare_promise_all() {
        let d = run_on("Promise.all([p1, p2]);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-floating-promise");
    }

    #[test]
    fn flags_async_looking_method() {
        let d = run_on("db.save(user);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_fetch() {
        let d = run_on("api.fetch(url);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_awaited_call() {
        // `await api.fetch(url);` parses as await_expression inside
        // expression_statement — not a bare call_expression.
        assert!(run_on("async function f() { await api.fetch(url); }").is_empty());
    }

    #[test]
    fn allows_then_chain() {
        assert!(run_on("api.fetch(url).then(handleResult);").is_empty());
    }

    #[test]
    fn allows_catch_chain() {
        assert!(run_on("api.fetch(url).catch(err);").is_empty());
    }

    #[test]
    fn allows_assignment() {
        // Assignment is not a bare expression_statement → call_expression.
        assert!(run_on("const p = api.fetch(url);").is_empty());
    }

    #[test]
    fn allows_non_async_looking_call() {
        // Not in our heuristic list — we conservatively skip.
        assert!(run_on("helper(x);").is_empty());
    }

    #[test]
    fn allows_void_expression() {
        // `void p;` parses as expression_statement → unary_expression, not a bare call.
        assert!(run_on("void api.fetch(url);").is_empty());
    }

    // Regression tests for issue #183: `.delete(...)` on Map/Set/WeakMap/WeakSet
    // returns `boolean`, not a Promise, and must not be flagged.

    #[test]
    fn allows_map_delete_in_for_of() {
        let src = "\
const cache = new Map();
for (const [key] of cache) {
  cache.delete(key);
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_set_delete() {
        assert!(run_on("set.delete(value);").is_empty());
    }

    #[test]
    fn allows_weakmap_delete() {
        assert!(run_on("weakMap.delete(obj);").is_empty());
    }

    #[test]
    fn allows_weakset_delete() {
        assert!(run_on("weakSet.delete(obj);").is_empty());
    }

    #[test]
    fn still_flags_genuine_promise_returning_member_call() {
        // Negative: a method name that genuinely looks async stays flagged.
        let d = run_on("repo.save(entity);");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #208: URLSearchParams mutator methods
    // (`delete`, `set`, `append`, `sort`) return `void` per WHATWG URL spec.

    #[test]
    fn allows_urlsearchparams_delete() {
        let src = "\
const params = new URLSearchParams(\"?a=1\");
params.delete(\"a\");
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_urlsearchparams_set() {
        assert!(run_on("params.set(\"a\", \"b\");").is_empty());
    }

    #[test]
    fn allows_urlsearchparams_append() {
        assert!(run_on("params.append(\"x\", \"y\");").is_empty());
    }

    #[test]
    fn allows_urlsearchparams_sort() {
        assert!(run_on("params.sort();").is_empty());
    }

    #[test]
    fn allows_url_searchparams_chain_delete() {
        let src = "\
const parsed = new URL(\"https://example.com/?a=1\");
parsed.searchParams.delete(\"a\");
";
        assert!(run_on(src).is_empty());
    }

    // Regression tests for issue #3377: `.commit()` and `.flush()` are dominated
    // by synchronous APIs, so both names were dropped from the heuristic.

    #[test]
    fn allows_void_commit_call() {
        assert!(run_on("entry.commit(to);").is_empty());
    }

    #[test]
    fn allows_void_flush_call() {
        assert!(run_on("scrollWaiter.flush();").is_empty());
    }

    #[test]
    fn still_flags_genuine_async_save_after_commit_flush_drop() {
        let d = run_on("repo.save(entity);");
        assert_eq!(d.len(), 1);
    }
}
