//! sql-require-search-path — SQL text backend for .sql migration files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !super::is_migration_path(ctx.path) {
            return vec![];
        }
        if crate::rules::sql_helpers::is_clickhouse_ddl(ctx.source) {
            return vec![];
        }
        if !super::has_search_path_dependent_ddl(ctx.source) {
            return vec![];
        }
        if super::sql_sets_search_path(ctx.source) {
            return vec![];
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: super::MESSAGE.into(),
            severity: Severity::Error,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), src))
    }

    #[test]
    fn flags_ddl_without_search_path() {
        assert_eq!(
            run("/app/migrations/001.sql", "CREATE TABLE users (id INT);").len(),
            1
        );
    }

    #[test]
    fn allows_ddl_with_search_path() {
        assert!(
            run(
                "/app/migrations/001.sql",
                "SET search_path = pg_catalog, public;\nCREATE TABLE users (id INT);"
            )
            .is_empty()
        );
    }

    #[test]
    fn message_prescribes_a_spelling_that_works_for_create_table_issue_8491() {
        // A leading `pg_catalog` makes `current_schema()` pg_catalog, so an
        // unqualified `CREATE TABLE` is refused; and a session-scoped `SET`
        // outlives the migration's COMMIT on a pooled connection. The remedy
        // the diagnostic hands the author must be neither.
        let diagnostics = run("/app/migrations/001.sql", "CREATE TABLE users (id INT);");
        let [only] = diagnostics.as_slice() else {
            panic!("expected exactly one diagnostic, got {diagnostics:?}");
        };
        assert!(only.message.contains("SET LOCAL search_path = public;"));
        assert!(!only.message.contains("pg_catalog, public"));
    }

    #[test]
    fn allows_transaction_scoped_search_path_issue_8512() {
        assert!(
            run(
                "/app/migrations/a.sql",
                "SET LOCAL lock_timeout = '5s';\n\
                 SET LOCAL search_path = pg_catalog, public;\n\
                 ALTER TABLE \"x\" ADD COLUMN \"y\" numeric;"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_fully_schema_qualified_migration_issue_8503() {
        assert!(
            run(
                "/app/migrations/001.sql",
                "SET LOCAL lock_timeout = '5s';\n\n\
                 ALTER TABLE public.\"objective\" ADD COLUMN \"unit\" text DEFAULT 'euros' NOT NULL;"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_schema_qualified_create_table_issue_8491() {
        assert!(
            run(
                "/app/migrations/20260831000000_article_group_model/migration.sql",
                "SET LOCAL lock_timeout = '5s';\n\n\
                 CREATE TABLE \"public\".\"article_group\" (\n\
                 \t\"id\" uuid PRIMARY KEY,\n\
                 \t\"name\" text NOT NULL\n\
                 );\n\
                 CREATE UNIQUE INDEX \"article_group_name_idx\" ON \"public\".\"article_group\" (\"name\");"
            )
            .is_empty()
        );
    }

    #[test]
    fn skips_non_migration() {
        assert!(run("/app/src/schema.sql", "CREATE TABLE users (id INT);").is_empty());
    }

    #[test]
    fn skips_clickhouse_migration_issue_7765() {
        assert!(
            run(
                "/app/migrations/003.sql",
                "CREATE TABLE Events (id UInt64) ENGINE = MergeTree ORDER BY id;"
            )
            .is_empty()
        );
    }
}
