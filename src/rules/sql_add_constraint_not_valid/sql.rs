//! sql-add-constraint-not-valid — SQL text backend for .sql migration files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return vec![];
        }
        if !super::sql_violates_add_constraint(ctx.source) {
            return vec![];
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "`ALTER TABLE ADD CONSTRAINT` without `NOT VALID` takes an AccessExclusiveLock — split into ADD ... NOT VALID then VALIDATE CONSTRAINT.".into(),
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
    fn flags_add_check_constraint_without_not_valid() {
        let src = "ALTER TABLE users ADD CONSTRAINT chk_age CHECK (age >= 0);";
        assert_eq!(run("/app/migrations/001.sql", src).len(), 1);
    }

    #[test]
    fn allows_add_constraint_with_not_valid() {
        let src = "ALTER TABLE users ADD CONSTRAINT chk_age CHECK (age >= 0) NOT VALID;";
        assert!(run("/app/migrations/001.sql", src).is_empty());
    }

    #[test]
    fn skips_non_migration() {
        let src = "ALTER TABLE users ADD CONSTRAINT chk_age CHECK (age >= 0);";
        assert!(run("/app/src/schema.sql", src).is_empty());
    }
}
