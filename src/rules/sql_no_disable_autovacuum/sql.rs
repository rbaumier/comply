//! sql-no-disable-autovacuum — SQL text backend for .sql files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !super::sql_disables_autovacuum(ctx.source) {
            return vec![];
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Disabling autovacuum causes bloat and XID wraparound — tune `autovacuum_vacuum_scale_factor` / `autovacuum_vacuum_threshold` instead.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), src))
    }

    #[test]
    fn flags_autovacuum_false() {
        let src = "ALTER TABLE users SET (autovacuum_enabled = false);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_autovacuum_off() {
        let src = "ALTER TABLE users SET (autovacuum_enabled = off);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_normal_table() {
        assert!(run("CREATE TABLE users (id INT);").is_empty());
    }
}
