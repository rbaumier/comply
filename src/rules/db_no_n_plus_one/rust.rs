//! db-no-n-plus-one Rust backend.
//!
//! Flag `.await` on DB-like calls inside loops. In Rust this looks like
//! `conn.query(...).await` inside `for`/`while`/`loop` blocks.
//!
//! A `while`/`loop` whose awaited query carries a SQL `LIMIT` clause is
//! keyset/chunk pagination — it reads one bounded page per iteration, the
//! opposite of an N+1 — and is exempt. A `for` binds each element of a
//! collection (one dependent query per element), so it stays flagged.
//!
//! Detection is AST-based. The awaited expression must be a `call_expression`
//! whose callee is a `field_expression` `<receiver>.<method>`. Unambiguous
//! sqlx driver methods (`fetch_one`/`fetch_all`/`fetch_optional`) flag on the
//! method name alone. Overloaded generic names (`query`/`execute`/`find`/
//! `insert`/`update`/`delete`) additionally require the receiver chain to be
//! anchored on a DB-like binding so a `HashMap::insert` or a GraphQL
//! `extensions.execute(..)` pipeline is not mistaken for a database query.
//!
//! Inline `#[cfg(test)]` modules are exempt: parametrized tests routinely
//! create a fresh in-memory datastore per loop iteration and run one query
//! against it, which is not the N+1 antipattern (each iteration has isolated
//! storage and cannot be batched). Path-based test files are handled by
//! `skip_in_test_dir`; this covers tests embedded in production `src/` files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;
use crate::rules::sql_helpers::contains_word;

/// Method names that are unambiguously sqlx/ORM driver calls — DB-specific by
/// name alone, so a match flags without any receiver anchoring.
const UNAMBIGUOUS_METHODS: &[&str] = &["fetch_one", "fetch_all", "fetch_optional"];

/// Method names that are heavily overloaded across the ecosystem (futures
/// executors, command runners, `HashMap::insert`, GraphQL pipelines, …). A
/// match on one of these flags only when the receiver chain is anchored on a
/// DB-like binding (see `DB_RECEIVER_NAMES`).
const GENERIC_METHODS: &[&str] = &["query", "execute", "find", "insert", "update", "delete"];

/// Binding/field names that signal a database handle. Matched case-insensitively
/// against either the receiver-chain root (`conn.execute(..)`) or the field the
/// method is called on (`self.pool.execute(..)` → `pool`).
const DB_RECEIVER_NAMES: &[&str] = &[
    "conn",
    "connection",
    "db",
    "database",
    "pool",
    "tx",
    "txn",
    "trx",
    "transaction",
    "client",
    "cursor",
    "session",
    "repo",
    "repository",
];

/// True if `node` (peeled of `.await`/`?`) is an awaited DB query call.
///
/// AST shape: a `call_expression` whose `function` is a `field_expression`
/// `<receiver>.<method>`. Unambiguous sqlx methods flag on the method name
/// alone. Generic, overloaded method names additionally require the receiver
/// chain to be anchored on a DB-like name — either the chain's root identifier
/// or the immediate receiver field the method is called on.
fn is_db_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    // Peel `?` / `.await` wrappers around the call.
    while matches!(current.kind(), "try_expression" | "await_expression") {
        let Some(inner) = current.named_child(0) else {
            return false;
        };
        current = inner;
    }
    if current.kind() != "call_expression" {
        return false;
    }
    let Some(function) = current.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let Some(method) = function
        .child_by_field_name("field")
        .and_then(|n| n.utf8_text(source).ok())
    else {
        return false;
    };

    if UNAMBIGUOUS_METHODS.contains(&method) {
        return true;
    }
    if !GENERIC_METHODS.contains(&method) {
        return false;
    }

    // Generic name: require a DB-like anchor on the receiver chain.
    let Some(receiver) = function.child_by_field_name("value") else {
        return false;
    };
    immediate_receiver_is_db_like(receiver, source) || receiver_root_is_db_like(receiver, source)
}

/// True if the receiver the method is called directly on carries a DB-like
/// name. Covers `self.pool.execute(..)` → field `pool`, and `pool.execute(..)`
/// → identifier `pool`.
fn immediate_receiver_is_db_like(receiver: tree_sitter::Node, source: &[u8]) -> bool {
    let name = match receiver.kind() {
        "field_expression" => receiver.child_by_field_name("field"),
        "identifier" => Some(receiver),
        _ => None,
    };
    name.and_then(|n| n.utf8_text(source).ok())
        .is_some_and(is_db_name)
}

/// Walk the receiver chain down to its root expression and test whether that
/// root identifier is DB-like (`conn.foo().bar().execute(..)` → `conn`).
fn receiver_root_is_db_like(receiver: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = receiver;
    loop {
        match current.kind() {
            "field_expression" => {
                let Some(value) = current.child_by_field_name("value") else {
                    return false;
                };
                current = value;
            }
            "call_expression" => {
                let Some(function) = current.child_by_field_name("function") else {
                    return false;
                };
                current = function;
            }
            "try_expression" | "await_expression" | "parenthesized_expression" => {
                let Some(inner) = current.named_child(0) else {
                    return false;
                };
                current = inner;
            }
            _ => break,
        }
    }
    current
        .utf8_text(source)
        .ok()
        .is_some_and(is_db_name)
}

fn is_db_name(name: &str) -> bool {
    DB_RECEIVER_NAMES
        .iter()
        .any(|db| db.eq_ignore_ascii_case(name))
}

/// The nearest loop enclosing an awaited expression.
enum EnclosingLoop {
    /// `for x in collection { … }` — per-item iteration over a collection, the
    /// structural shape of an N+1 (one dependent query per element).
    For,
    /// `while cond { … }` / `loop { … }` — no per-item collection binding; may
    /// be a keyset/chunk-pagination fetch of one bounded page per iteration.
    WhileOrLoop,
}

/// Classify the nearest loop enclosing `node`, stopping at the nearest
/// function/closure boundary. `None` when the await is not inside a loop.
fn enclosing_loop(node: tree_sitter::Node) -> Option<EnclosingLoop> {
    let mut parent = node.parent();
    while let Some(p) = parent {
        match p.kind() {
            "for_expression" => return Some(EnclosingLoop::For),
            "while_expression" | "loop_expression" => return Some(EnclosingLoop::WhileOrLoop),
            "function_item" | "closure_expression" => return None,
            _ => {}
        }
        parent = p.parent();
    }
    None
}

/// True if any string literal in the awaited call's subtree carries a SQL
/// `LIMIT` clause (word-boundary match, so a `rate_limits` table name does not
/// count). A `LIMIT`-bounded fetch reads one bounded page per iteration —
/// keyset/chunk pagination — not the single dependent row of an N+1.
fn awaited_query_has_limit(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if matches!(current.kind(), "string_literal" | "raw_string_literal")
            && let Ok(text) = current.utf8_text(source)
            && contains_word(&text.to_ascii_lowercase(), "limit")
        {
            return true;
        }
        stack.extend(current.children(&mut cursor));
    }
    false
}

crate::ast_check! { on ["await_expression"] => |node, source, ctx, diagnostics|
    let Some(loop_kind) = enclosing_loop(node) else {
        return;
    };

    if is_in_test_context(node, source) {
        return;
    }

    let Some(inner) = node.named_child(0) else { return };
    if !is_db_call(inner, source) {
        return;
    }

    // Keyset/chunk pagination: a `while`/`loop` whose awaited query is bounded
    // by a SQL `LIMIT` fetches one page per iteration — a deliberate batching
    // strategy, the opposite of an N+1 — so it is not flagged. A `for` binds
    // each element of a collection and stays flagged.
    if matches!(loop_kind, EnclosingLoop::WhileOrLoop) && awaited_query_has_limit(inner, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "db-no-n-plus-one".into(),
        message: "Awaited DB query inside a loop — use a batch query or JOIN.".into(),
        severity: Severity::Error,
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_query_in_loop() {
        let src = "async fn f(ids: Vec<i32>) { for id in ids { db.query(id).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_query_outside_loop() {
        let src = "async fn f() { db.query(1).await; }";
        assert!(run_on(src).is_empty());
    }

    // Issue #1470: parametrized tests create a fresh in-memory datastore per
    // loop iteration and run one query against it — not an N+1 query. An inline
    // `#[cfg(test)]` module in a production `src/` file must be exempt.
    #[test]
    fn allows_query_in_loop_inside_cfg_test_module() {
        let src = r#"
            #[cfg(test)]
            mod tests {
                async fn t() {
                    for level in &test_levels {
                        for case in &test_cases {
                            let ds = Datastore::new("memory").await.unwrap();
                            ds.execute(&query, &sess, None).await.unwrap();
                        }
                    }
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    // Issue #1470: a `tests/`-dir path is suppressed by `skip_in_test_dir`.
    // Gated run honours the production `applies_to_file` gate.
    #[test]
    fn allows_query_in_loop_in_tests_dir() {
        let src = "async fn f(ids: Vec<i32>) { for id in ids { db.query(id).await; } }";
        let diags =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "crate/tests/signin.rs");
        assert!(diags.is_empty());
    }

    // Negative space: the same loop query in a production (non-test) path still
    // fires — the exemption is test-scoped, the rule still catches real N+1.
    #[test]
    fn flags_query_in_loop_in_production_path() {
        let src = "async fn f(ids: Vec<i32>) { for id in ids { db.query(id).await; } }";
        let diags =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "crate/src/iam/signin.rs");
        assert_eq!(diags.len(), 1);
    }

    // Issue #3964: a GraphQL extension pipeline `ctx_field.query_env.extensions
    // .execute(..).await` is a per-field resolver run, not a DB query. The
    // receiver chain (`ctx_field` root, called on `extensions`) is not DB-like,
    // so the overloaded `execute` name must not anchor.
    #[test]
    fn allows_graphql_extension_execute_in_loop() {
        let src = r#"async fn f() {
            for f in fields {
                let resp = ctx_field
                    .query_env
                    .extensions
                    .execute(ctx_field.query_env.operation_name.as_deref(), f)
                    .await;
            }
        }"#;
        assert!(run_on(src).is_empty());
    }

    // Issue #3263 facet: `HashMap::insert` shares the overloaded `insert` name
    // but its receiver (`map`) is not DB-like, so it must not flag.
    #[test]
    fn allows_hashmap_insert_in_loop() {
        let src = "async fn f() { for (k, v) in pairs { map.insert(k, v); } }";
        assert!(run_on(src).is_empty());
    }

    // True positive: `conn` is a DB-like root → overloaded `execute` anchors.
    #[test]
    fn flags_conn_execute_in_loop() {
        let src = "async fn f(ids: Vec<i32>) { for id in ids { conn.execute(sql).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // True positive: DB-like field on `self` (`self.pool`) anchors the generic
    // name even though the chain root is `self`.
    #[test]
    fn flags_self_pool_query_in_loop() {
        let src =
            "async fn f(&self, ids: Vec<i32>) { for id in ids { self.pool.query(id).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // True positive: unambiguous sqlx method flags without any receiver anchor.
    #[test]
    fn flags_unambiguous_fetch_all_in_loop() {
        let src =
            "async fn f(ids: Vec<i32>) { for id in ids { build_query(id).fetch_all(ex).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #7892: keyset/chunk pagination — a bare `loop {}` fetching one
    // `LIMIT`-bounded page per iteration and breaking on a short chunk is a
    // batching strategy, the opposite of an N+1. The `LIMIT` clause lives in the
    // `sqlx::query!` macro string within the awaited call's subtree.
    #[test]
    fn allows_keyset_pagination_loop_with_limit() {
        let src = r##"async fn f(&self) {
            loop {
                let rows = sqlx::query!(
                    r#"
                    SELECT storage_logs.hashed_key, storage_logs.value
                    FROM storage_logs
                    WHERE storage_logs.hashed_key >= $2::bytea
                    ORDER BY storage_logs.hashed_key
                    LIMIT $4
                    "#,
                    QUERY_LIMIT as i32
                )
                .fetch_all(self.storage)
                .await
                .unwrap();
                if rows.len() < QUERY_LIMIT {
                    break;
                }
            }
        }"##;
        assert!(run_on(src).is_empty());
    }

    // Issue #7892: a `loop` fetching a `LIMIT`-bounded chunk passed as a string
    // argument is likewise chunk pagination, not an N+1.
    #[test]
    fn allows_loop_fetch_all_with_limit_arg() {
        let src = r#"async fn f() {
            loop {
                let page = db.fetch_all("SELECT * FROM t ORDER BY id LIMIT 100").await;
                if page.len() < 100 { break; }
            }
        }"#;
        assert!(run_on(src).is_empty());
    }

    // Issue #7892: the exemption is `LIMIT`-gated, not a blanket `loop`/`while`
    // pass. A `while let` popping items and running one dependent query each,
    // with no `LIMIT`, is a genuine N+1 and still fires.
    #[test]
    fn flags_while_let_pop_query_without_limit() {
        let src =
            "async fn f(mut ids: Vec<i32>) { while let Some(id) = ids.pop() { db.query(id).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #7892: a `loop` whose awaited query has no `LIMIT` is not chunk
    // pagination and still fires — proving suppression requires the clause.
    #[test]
    fn flags_loop_query_without_limit() {
        let src = "async fn f(sql: &str) { loop { db.fetch_all(sql).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
