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
//! `insert`/`update`/`delete`) additionally require two independent signals:
//! the receiver chain anchored on a DB-like binding, so a `HashMap::insert` or
//! a GraphQL `extensions.execute(..)` pipeline is not mistaken for a query, and
//! crate provenance for that receiver, so an HTTP, gRPC or object-storage client
//! sharing one of those binding names is not read as a database handle.
//!
//! Provenance is decided per receiver, from the crate that declares the
//! receiver's type ([`rust_helpers::receiver_type_origins`]): a `tonic::Channel`
//! and a `tokio_postgres::Client` may both be called `client`, and only the
//! second anchors. A receiver whose type is declared by the linted crate itself
//! is decided by the module that declares it — a crate-local `db` module
//! re-exporting a pool carries the provenance its `crate::`-rooted path hides.
//! When the file declares no type for the receiver, provenance falls back to the
//! file's own imports and qualified paths, plus the linted package's identity: a
//! database crate's own source reaches its handles through `crate::` paths that
//! name no crate at all.
//!
//! Inline `#[cfg(test)]` modules are exempt: parametrized tests routinely
//! create a fresh in-memory datastore per loop iteration and run one query
//! against it, which is not the N+1 antipattern (each iteration has isolated
//! storage and cannot be batched). Path-based test files are handled by
//! `skip_in_test_dir`; this covers tests embedded in production `src/` files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::CheckCtx;
use crate::rules::rust_helpers::{
    TypeOrigin, file_references_db_crate, import_source_references_db_crate, is_db_crate,
    is_test_code, receiver_type_origins,
};
use crate::rules::sql_helpers::contains_word;

/// Method names that are unambiguously sqlx/ORM driver calls — DB-specific by
/// name alone, so a match flags without any receiver anchoring.
const UNAMBIGUOUS_METHODS: &[&str] = &["fetch_one", "fetch_all", "fetch_optional"];

/// Method names that are heavily overloaded across the ecosystem (futures
/// executors, command runners, `HashMap::insert`, GraphQL pipelines, …). A
/// match on one of these flags only when the receiver chain is anchored on a
/// DB-like binding (see `DB_RECEIVER_NAMES`) *and* that receiver carries
/// database provenance (see [`receiver_is_db_handle`]).
const GENERIC_METHODS: &[&str] = &["query", "execute", "find", "insert", "update", "delete"];

/// Binding/field names a database handle can carry. Matched case-insensitively
/// against either the receiver-chain root (`conn.execute(..)`) or the field the
/// method is called on (`self.pool.execute(..)` → `pool`).
///
/// These names are shared with non-database handles — `client` alone covers
/// `reqwest::Client`, `tonic` gRPC channels and S3 clients as much as
/// `tokio_postgres::Client` — so a match here narrows the candidates but never
/// establishes that the receiver is a database handle: the crate-provenance
/// gate in [`receiver_is_db_handle`] decides that.
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
/// or the immediate receiver field the method is called on — and that receiver
/// to carry database provenance ([`receiver_is_db_handle`]). The binding name is
/// chosen by the author and says nothing about what the value is; the crate the
/// receiver's type comes from is what makes the call a database query.
fn is_db_call(node: tree_sitter::Node, source: &[u8], ctx: &CheckCtx) -> bool {
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
    if !immediate_receiver_is_db_like(receiver, source)
        && !receiver_root_is_db_like(receiver, source)
    {
        return false;
    }

    // …and database provenance for that receiver, so the anchor names a database
    // handle rather than an HTTP/gRPC/object-storage client sharing the name.
    receiver_is_db_handle(receiver, source, ctx)
}

/// True if the anchored `receiver` is a database handle.
///
/// The evidence is the crate that declares the receiver's type, resolved in the
/// file ([`receiver_type_origins`]):
/// - a database crate declares it → a database handle;
/// - the linted crate declares it → the module that declares the type answers,
///   since a `crate::`-rooted path names no crate of its own;
/// - external crates alone declare it → not a database handle, whatever else the
///   file talks to;
/// - the file declares no type for the receiver → the file's own provenance
///   answers, as it does for every receiver it cannot resolve.
fn receiver_is_db_handle(receiver: tree_sitter::Node, source: &[u8], ctx: &CheckCtx) -> bool {
    let origins = receiver_type_origins(receiver, source);
    if origins
        .iter()
        .any(|origin| matches!(origin, TypeOrigin::ExternalCrate(name) if is_db_crate(name)))
    {
        return true;
    }
    if let Some(local) = origins.iter().find_map(|origin| match origin {
        TypeOrigin::CrateLocal(name) => Some(name),
        _ => None,
    }) {
        return import_source_references_db_crate(ctx.project, ctx.path, local)
            || file_reaches_db_crate(receiver, source, ctx);
    }
    if !origins.is_empty()
        && origins
            .iter()
            .all(|origin| matches!(origin, TypeOrigin::ExternalCrate(_)))
    {
        return false;
    }
    file_reaches_db_crate(receiver, source, ctx)
}

/// True if the file containing `node` talks to a database at file granularity:
/// it imports or qualifies a database crate, or the package it belongs to is
/// itself one — a database crate reaches its own handles through `crate::` paths,
/// whose leftmost segment is never a crate name.
fn file_reaches_db_crate(node: tree_sitter::Node, source: &[u8], ctx: &CheckCtx) -> bool {
    file_references_db_crate(node, source)
        || ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|manifest| {
                manifest
                    .crate_identifier()
                    .is_some_and(|name| is_db_crate(&name))
            })
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

    if is_test_code(node, source, ctx) {
        return;
    }

    let Some(inner) = node.named_child(0) else { return };
    if !is_db_call(inner, source, ctx) {
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

    /// Manifest of an application crate — the package is not itself a database
    /// crate, so only its files' own provenance can anchor a query.
    const APP_CARGO_TOML: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

    /// Crate root declaring the two modules below it, so the module graph can
    /// resolve a `use crate::db::…` to the file that backs `crate::db`.
    const CRATE_ROOT: &str = "pub mod db;\npub mod handler;\n";

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_query_in_loop() {
        let src = "use sqlx::PgPool;
            async fn f(ids: Vec<i32>) { for id in ids { db.query(id).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_query_outside_loop() {
        let src = "use sqlx::PgPool;
            async fn f() { db.query(1).await; }";
        assert!(run_on(src).is_empty());
    }

    // Issue #1470: parametrized tests create a fresh in-memory datastore per
    // loop iteration and run one query against it — not an N+1 query. An inline
    // `#[cfg(test)]` module in a production `src/` file must be exempt.
    #[test]
    fn allows_query_in_loop_inside_cfg_test_module() {
        let src = r#"
            use surrealdb::Datastore;

            #[cfg(test)]
            mod tests {
                async fn t() {
                    for level in &test_levels {
                        for case in &test_cases {
                            let db = Datastore::new("memory").await.unwrap();
                            db.execute(&query, &sess, None).await.unwrap();
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
        let src = "use sqlx::PgPool;
            async fn f(ids: Vec<i32>) { for id in ids { db.query(id).await; } }";
        let diags =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "crate/tests/signin.rs");
        assert!(diags.is_empty());
    }

    // Negative space: the same loop query in a production (non-test) path still
    // fires — the exemption is test-scoped, the rule still catches real N+1.
    #[test]
    fn flags_query_in_loop_in_production_path() {
        let src = "use sqlx::PgPool;
            async fn f(ids: Vec<i32>) { for id in ids { db.query(id).await; } }";
        let diags =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "crate/src/iam/signin.rs");
        assert_eq!(diags.len(), 1);
    }

    // Issue #3964: a GraphQL extension pipeline `ctx_field.query_env.extensions
    // .execute(..).await` is a per-field resolver run, not a DB query. The
    // receiver chain (`ctx_field` root, called on `extensions`) is not DB-like,
    // so the overloaded `execute` name must not anchor — even in a resolver file
    // that does reach a database crate.
    #[test]
    fn allows_graphql_extension_execute_in_loop() {
        let src = r#"use sqlx::PgPool;
        async fn f() {
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
    // but its receiver (`map`) is not DB-like, so it must not flag — even in a
    // file that does reach a database crate.
    #[test]
    fn allows_hashmap_insert_in_loop() {
        let src = "use sqlx::PgPool;
            async fn f() { for (k, v) in pairs { map.insert(k, v); } }";
        assert!(run_on(src).is_empty());
    }

    // True positive: `conn` is a DB-like root in a file that reaches a database
    // crate → overloaded `execute` anchors.
    #[test]
    fn flags_conn_execute_in_loop() {
        let src = "use diesel_async::AsyncPgConnection;
            async fn f(ids: Vec<i32>) { for id in ids { conn.execute(sql).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // True positive: DB-like field on `self` (`self.pool`) anchors the generic
    // name even though the chain root is `self`.
    #[test]
    fn flags_self_pool_query_in_loop() {
        let src = "use sqlx::PgPool;
            async fn f(&self, ids: Vec<i32>) { for id in ids { self.pool.query(id).await; } }";
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
        let src = "use sqlx::PgPool;
            async fn f(mut ids: Vec<i32>) { while let Some(id) = ids.pop() { db.query(id).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #7892: a `loop` whose awaited query has no `LIMIT` is not chunk
    // pagination and still fires — proving suppression requires the clause.
    #[test]
    fn flags_loop_query_without_limit() {
        let src = "async fn f(sql: &str) { loop { db.fetch_all(sql).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #6856 (astral-sh/uv `crates/uv-client/src/base_client.rs`): a
    // `reqwest_middleware` client re-executes a request in a loop to follow
    // redirects itself. `client` is a name a database handle can carry too, but
    // the file reaches no database crate, so the receiver is an HTTP client and
    // the call is not a query.
    #[test]
    fn allows_http_client_execute_in_loop() {
        let src = r#"
            use reqwest_middleware::ClientWithMiddleware;

            impl BaseClient {
                async fn execute_with_redirect_handling(
                    &self,
                    req: Request,
                ) -> reqwest_middleware::Result<Response> {
                    let mut request = req;
                    loop {
                        let result = self
                            .client
                            .execute(request.try_clone().expect("HTTP request must be cloneable"))
                            .await;
                        request = redirect_request(&result)?;
                    }
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    // Issue #6856: a `#[cfg(test)]` gate keeps its import out of the release
    // build, so it says nothing about the production code beside it and the
    // HTTP loop stays silent — whether the gate sits on a test module or
    // directly on the import.
    #[test]
    fn allows_http_client_execute_in_loop_beside_cfg_test_module_import() {
        let src = r#"
            use reqwest_middleware::ClientWithMiddleware;

            impl BaseClient {
                async fn send_all(&self, requests: Vec<Request>) {
                    for req in requests {
                        self.client.execute(req).await;
                    }
                }
            }

            #[cfg(test)]
            mod tests {
                use sqlx::PgPool;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_http_client_execute_in_loop_beside_cfg_test_gated_import() {
        let src = r#"
            use reqwest_middleware::ClientWithMiddleware;
            #[cfg(test)]
            use sqlx::PgPool;

            impl BaseClient {
                async fn send_all(&self, requests: Vec<Request>) {
                    for req in requests {
                        self.client.execute(req).await;
                    }
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    // Issue #6856, negative space: `tokio_postgres` names its own connection
    // handle `client`, so the identical receiver name in a file that reaches a
    // database crate is a database handle and the N+1 still fires. The fix is
    // the crate-provenance gate, not the removal of an ambiguous name.
    #[test]
    fn flags_postgres_client_query_in_loop() {
        let src = r#"
            use tokio_postgres::Client;

            async fn load(client: &Client, ids: Vec<i32>) {
                for id in ids {
                    client.query("SELECT * FROM users WHERE id = $1", &[&id]).await;
                }
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #8280: a file that talks to a database does not make every receiver
    // in it a database handle. The gRPC channel field carries a name a database
    // handle can carry too, but its declared type comes from `tonic`, so the
    // awaited `execute` is a remote call, not a query.
    #[test]
    fn allows_grpc_client_execute_in_loop_beside_db_pool_field() {
        let src = r#"
            use sqlx::PgPool;
            use tonic::transport::Channel;

            struct Gateway {
                client: Channel,
                pool: PgPool,
            }

            impl Gateway {
                async fn forward_all(&self, requests: Vec<Request>) {
                    for req in requests {
                        self.client.execute(req).await;
                    }
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    // Issue #8280, negative space: the same file, same method name, the other
    // field. `self.pool` is declared `sqlx::PgPool`, so the N+1 still fires —
    // the fix is per-receiver provenance, not distrust of the file.
    #[test]
    fn flags_db_pool_execute_in_loop_beside_grpc_client_field() {
        let src = r#"
            use sqlx::PgPool;
            use tonic::transport::Channel;

            struct Gateway {
                client: Channel,
                pool: PgPool,
            }

            impl Gateway {
                async fn touch_all(&self, ids: Vec<i32>) {
                    for id in ids {
                        self.pool.execute(build(id)).await;
                    }
                }
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #8278 (SeaQL/sea-orm `src/rbac/schema.rs`): a database crate's own
    // source reaches its connection handle through `crate::` paths, whose root
    // segment is never a database crate name. The linted package is itself a
    // database crate, which is the provenance those paths hide.
    #[test]
    fn flags_generic_connection_execute_in_loop_in_database_crate() {
        let cargo = "[package]\nname = \"sea-orm\"\nversion = \"1.1.0\"\nedition = \"2021\"\n";
        let src = r#"
            use crate::{ConnectionTrait, DbErr, EntityTrait, Schema};

            async fn create_indexes<C, E>(db: &C, entity: E, schema: &Schema) -> Result<(), DbErr>
            where
                C: ConnectionTrait,
                E: EntityTrait,
            {
                for stmt in schema.create_index_from_entity(entity) {
                    db.execute(&stmt).await?;
                }
                Ok(())
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule_with_cargo(
            &Check,
            cargo,
            src,
            "src/rbac/schema.rs",
        );
        assert_eq!(diags.len(), 1);
    }

    // Issue #8278, application shape: the handle is a crate-local type re-exported
    // by a database module. `crate::db::DbPool` roots at `crate`, so the file
    // itself names no database crate; the module declaring the type does.
    #[test]
    fn flags_pool_execute_in_loop_through_crate_local_db_module() {
        let diags = crate::rules::test_helpers::run_rule_in_indexed_crate(
            &Check,
            &[
                ("Cargo.toml", APP_CARGO_TOML),
                ("src/lib.rs", CRATE_ROOT),
                ("src/db.rs", "use sqlx::PgPool;\npub struct DbPool(PgPool);\n"),
                (
                    "src/handler.rs",
                    r"
                    use crate::db::DbPool;

                    async fn touch(pool: &DbPool, ids: Vec<i32>) {
                        for id in ids {
                            pool.execute(build(id)).await;
                        }
                    }
                    ",
                ),
            ],
        );
        assert_eq!(diags.len(), 1);
    }

    // Issue #8278, negative space: the same crate-local shape whose declaring
    // module reaches no database crate is an in-memory pool, not a database
    // handle, and stays silent.
    #[test]
    fn allows_pool_execute_in_loop_through_crate_local_non_db_module() {
        let diags = crate::rules::test_helpers::run_rule_in_indexed_crate(
            &Check,
            &[
                ("Cargo.toml", APP_CARGO_TOML),
                ("src/lib.rs", CRATE_ROOT),
                (
                    "src/db.rs",
                    "use std::collections::HashMap;\npub struct DbPool(HashMap<u32, u32>);\n",
                ),
                (
                    "src/handler.rs",
                    r"
                    use crate::db::DbPool;

                    async fn touch(pool: &DbPool, ids: Vec<i32>) {
                        for id in ids {
                            pool.execute(build(id)).await;
                        }
                    }
                    ",
                ),
            ],
        );
        assert!(diags.is_empty());
    }

    // Issue #6856: a qualified path is provenance on its own — `sqlx::query(..)`
    // needs no `use` — so the awaited `self.pool.execute(..)` still flags.
    #[test]
    fn flags_pool_execute_with_qualified_sqlx_path_in_loop() {
        let src = r#"
            impl Store {
                async fn touch(&self, ids: Vec<i32>) {
                    for id in ids {
                        self.pool
                            .execute(sqlx::query("UPDATE t SET seen = now() WHERE id = $1").bind(id))
                            .await;
                    }
                }
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }
}
