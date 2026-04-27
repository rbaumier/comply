//! sql-no-truncate-in-app — SQL text backend for .sql files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !super::sql_uses_truncate(ctx.source) {
            return vec![];
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "`TRUNCATE` bypasses triggers, FK checks and audit — use `DELETE FROM table` instead.".into(),
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
    fn flags_truncate() {
        assert_eq!(run("TRUNCATE TABLE users;").len(), 1);
    }

    #[test]
    fn allows_delete() {
        assert!(run("DELETE FROM users;").is_empty());
    }
}
