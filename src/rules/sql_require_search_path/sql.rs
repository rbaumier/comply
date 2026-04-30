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
        if !super::sql_creates_or_alters_table(ctx.source) {
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
            message: "Migration must `SET search_path = pg_catalog, public;` (or use schema-qualified names) to prevent identifier hijacking.".into(),
            severity: Severity::Warning,
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
    fn skips_non_migration() {
        assert!(run("/app/src/schema.sql", "CREATE TABLE users (id INT);").is_empty());
    }
}
