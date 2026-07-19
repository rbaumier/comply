//! migration-needs-lock-timeout — Rust backend.
//!
//! Scoped to migration files. Walks Rust string literals; a string holding a
//! complete DDL statement without `SET lock_timeout` is flagged. The whole
//! file is skipped when any of its SQL strings is ClickHouse DDL — ClickHouse
//! has no `lock_timeout` setting — so a marker-less DDL string in a ClickHouse
//! migration is not flagged alongside its marked siblings.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;

#[derive(Default)]
struct State {
    /// Some SQL string in this file is ClickHouse DDL, so a Postgres-only
    /// `SET lock_timeout` must not be recommended for any string in it.
    any_clickhouse: bool,
    /// DDL strings lacking a lock timeout, emitted in `finish` once the whole
    /// file is known not to be ClickHouse.
    candidates: Vec<(usize, usize, usize, usize)>, // line, col, byte_start, byte_len
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(RUST_STRING_KINDS)
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::<State>::default())
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return;
        }
        let Ok(text) = node.utf8_text(ctx.source.as_bytes()) else {
            return;
        };
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        if crate::rules::sql_helpers::is_clickhouse_ddl(text) {
            state.any_clickhouse = true;
            return;
        }
        if !super::contains_ddl(text) || super::declares_lock_timeout(text) {
            return;
        }
        let pos = node.start_position();
        let range = node.byte_range();
        state
            .candidates
            .push((pos.row + 1, pos.column + 1, range.start, range.len()));
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast::<State>().ok()) else {
            return;
        };
        if state.any_clickhouse {
            return;
        }
        for (line, column, byte_start, byte_len) in state.candidates {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "DDL without `SET lock_timeout` — add `SET lock_timeout = '5s';` at the top to prevent write queue pileups.".into(),
                severity: Severity::Error,
                span: Some((byte_start, byte_len)),
            });
        }
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
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "/app/migrations/001.rs")
    }

    fn run_non_migration(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_alter_table_without_lock_timeout() {
        let src = r#"fn f() { let m = "ALTER TABLE users ADD COLUMN age INT"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_alter_table_with_lock_timeout() {
        let src = r#"fn f() { let m = "SET lock_timeout = '5s'; ALTER TABLE users ADD COLUMN age INT"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_migration_path() {
        let src = r#"fn f() { let m = "ALTER TABLE users ADD COLUMN age INT"; }"#;
        assert!(run_non_migration(src).is_empty());
    }

    #[test]
    fn ignores_non_ddl_string() {
        let src = r#"fn f() { let s = "this is just prose"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_query_builder_fragments_issue_1498() {
        // diesel diff_schema.rs: DDL assembled incrementally via a query
        // builder. Each literal is a keyword prefix with no target name and
        // no standalone statement to attach a lock_timeout to.
        let src = r#"
            fn generate_add_column(query_builder: &mut QueryBuilder, table: &str) {
                query_builder.push_sql("ALTER TABLE ");
                query_builder.push_identifier(table);
                query_builder.push_sql(" ADD COLUMN ");
                query_builder.push_sql(";");
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_complete_alter_table_statement_in_string() {
        // Negative-space guard: a genuine raw ALTER TABLE statement without a
        // lock_timeout must STILL fire.
        let src = r#"fn f() { let m = "ALTER TABLE orders ADD COLUMN total NUMERIC"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_clickhouse_alter_with_type_wrapper_issue_7765() {
        // ClickHouse ALTER TABLE with a Nullable(DateTime64(...)) column type:
        // `SET lock_timeout` is not a valid ClickHouse setting.
        let src = r#"fn f() { let m = "ALTER TABLE ChatInferenceDatapoint ADD COLUMN IF NOT EXISTS staled_at Nullable(DateTime64(6, 'UTC'))"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_clickhouse_modify_column_and_on_cluster_issue_7765() {
        let src = r#"fn f() {
            let a = "ALTER TABLE X ON CLUSTER c MODIFY COLUMN c LowCardinality(String)";
        }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_marker_less_clickhouse_string_via_file_level_gate_issue_7765() {
        // A marker-less `ALTER TABLE X ADD COLUMN y String` in a ClickHouse
        // migration file must NOT be flagged: a sibling string carries the
        // MergeTree engine marker, classifying the whole file as ClickHouse.
        let src = r#"fn f() {
            let create = "CREATE TABLE Events (id UInt64) ENGINE = MergeTree ORDER BY id";
            let alter = "ALTER TABLE Events ADD COLUMN name String";
        }"#;
        assert!(run(src).is_empty());
    }
}
