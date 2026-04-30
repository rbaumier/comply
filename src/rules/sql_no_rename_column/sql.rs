//! sql-no-rename-column — SQL text backend for .sql migration files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return vec![];
        }
        if !super::sql_renames_column(ctx.source) {
            return vec![];
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "`ALTER TABLE ... RENAME COLUMN` breaks running deploys — use expand-contract (add new column, dual-write, backfill, drop old).".into(),
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
    fn flags_rename_column_in_migration() {
        assert_eq!(
            run(
                "/app/migrations/001.sql",
                "ALTER TABLE users RENAME COLUMN name TO full_name;"
            )
            .len(),
            1
        );
    }

    #[test]
    fn skips_non_migration() {
        assert!(
            run(
                "/app/src/schema.sql",
                "ALTER TABLE users RENAME COLUMN name TO full_name;"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_other_alters() {
        assert!(
            run(
                "/app/migrations/001.sql",
                "ALTER TABLE users ADD COLUMN age INT;"
            )
            .is_empty()
        );
    }
}
