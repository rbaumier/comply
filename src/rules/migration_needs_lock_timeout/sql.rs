//! migration-needs-lock-timeout — SQL backend.
//!
//! For `.sql` migration files: check the raw content for DDL keywords.
//! No AST needed — the whole file is SQL.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return vec![];
        }
        if !super::contains_ddl(ctx.source) {
            return vec![];
        }
        if super::declares_lock_timeout(ctx.source) {
            return vec![];
        }
        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "DDL without `SET lock_timeout` — add `SET lock_timeout = '5s';` at the top to prevent write queue pileups.".into(),
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
        let ctx = CheckCtx::for_test(Path::new(path), src);
        Check.check(&ctx)
    }

    #[test]
    fn flags_ddl_without_lock_timeout() {
        let diags = run(
            "/app/migrations/001_add_col.sql",
            "ALTER TABLE users ADD COLUMN age INT;",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_ddl_with_lock_timeout() {
        let diags = run(
            "/app/migrations/001_add_col.sql",
            "SET lock_timeout = '5s';\nALTER TABLE users ADD COLUMN age INT;",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_non_migration_path() {
        let diags = run(
            "/app/src/schema.sql",
            "ALTER TABLE users ADD COLUMN age INT;",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_non_ddl_sql() {
        let diags = run(
            "/app/migrations/002_seed.sql",
            "INSERT INTO users (name) VALUES ('alice');",
        );
        assert!(diags.is_empty());
    }
}
