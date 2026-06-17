//! db-no-n-plus-one Rust backend.
//!
//! Flag `.await` on DB-like calls inside loops. In Rust this looks like
//! `conn.query(...).await` inside `for`/`while`/`loop` blocks.
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

fn is_inside_loop(node: tree_sitter::Node) -> bool {
    let mut parent = node.parent();
    while let Some(p) = parent {
        match p.kind() {
            "for_expression" | "while_expression" | "loop_expression" => return true,
            "function_item" | "closure_expression" => return false,
            _ => {}
        }
        parent = p.parent();
    }
    false
}

crate::ast_check! { on ["await_expression"] => |node, source, ctx, diagnostics|
    if !is_inside_loop(node) {
        return;
    }

    if is_in_test_context(node, source) {
        return;
    }

    let Some(inner) = node.named_child(0) else { return };
    if !is_db_call(inner, source) {
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
}
