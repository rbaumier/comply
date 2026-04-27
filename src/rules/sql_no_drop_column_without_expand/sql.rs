//! sql-no-drop-column-without-expand — SQL text backend for .sql migration files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return vec![];
        }
        if !super::sql_drops_column(ctx.source) {
            return vec![];
        }
        if super::file_marks_deprecation(ctx.source) {
            return vec![];
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "`DROP COLUMN` without a prior deprecation release breaks running deploys — mark the column unused in a previous release, then drop in a later migration.".into(),
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
    fn flags_drop_column_in_migration() {
        assert_eq!(
            run(
                "/app/migrations/001.sql",
                "ALTER TABLE users DROP COLUMN nickname;"
            )
            .len(),
            1
        );
    }

    #[test]
    fn skips_non_migration() {
        assert!(run(
            "/app/src/schema.sql",
            "ALTER TABLE users DROP COLUMN nickname;"
        )
        .is_empty());
    }

    #[test]
    fn allows_drop_with_deprecation_marker() {
        let src = "-- deprecated in v1.2\nALTER TABLE users DROP COLUMN nickname;";
        assert!(run("/app/migrations/001.sql", src).is_empty());
    }
}
