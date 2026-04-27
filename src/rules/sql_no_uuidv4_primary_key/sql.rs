//! sql-no-uuidv4-primary-key — `.sql` file backend.
//!
//! Pure SQL files are entirely SQL, so we apply `sql_uses_uuidv4_pk`
//! directly to the file contents. We restrict the check to migration
//! paths so that ad-hoc `.sql` snippets (seeds, fixtures, queries)
//! don't trigger.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return vec![];
        }
        if !super::sql_uses_uuidv4_pk(ctx.source) {
            return vec![];
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "UUIDv4 primary keys fragment B-tree indexes — use UUIDv7 or BIGINT IDENTITY instead.".into(),
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
    fn flags_gen_random_uuid_pk_in_migration() {
        let src = "CREATE TABLE users (id UUID PRIMARY KEY DEFAULT gen_random_uuid());";
        assert_eq!(run("migrations/0001_init.sql", src).len(), 1);
    }

    #[test]
    fn flags_uuid_generate_v4_pk_in_migration() {
        let src = "CREATE TABLE users (id UUID PRIMARY KEY DEFAULT uuid_generate_v4());";
        assert_eq!(run("migrations/0001_init.sql", src).len(), 1);
    }

    #[test]
    fn ignores_non_migration_paths() {
        let src = "CREATE TABLE users (id UUID PRIMARY KEY DEFAULT gen_random_uuid());";
        assert!(run("seeds/users.sql", src).is_empty());
    }

    #[test]
    fn ignores_uuid_without_pk() {
        let src = "CREATE TABLE users (id UUID DEFAULT gen_random_uuid());";
        assert!(run("migrations/0001_init.sql", src).is_empty());
    }
}
