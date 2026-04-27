//! sql-no-is-deleted-boolean — `.sql` file backend.
//!
//! Walks each statement in the file and applies `sql_uses_is_deleted_boolean`
//! against statements that look like DDL. Splitting on `;` keeps the DDL
//! filter accurate per-statement so unrelated DML in the same file does
//! not poison the check.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::sql_helpers::is_sql_ddl;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut start_line: usize = 1;
        let mut buf = String::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if buf.is_empty() {
                start_line = idx + 1;
            }
            buf.push_str(line);
            buf.push('\n');
            if line.contains(';') {
                if is_sql_ddl(&buf) && super::sql_uses_is_deleted_boolean(&buf) {
                    diagnostics.push(make_diag(ctx, start_line));
                }
                buf.clear();
            }
        }
        if !buf.trim().is_empty()
            && is_sql_ddl(&buf)
            && super::sql_uses_is_deleted_boolean(&buf)
        {
            diagnostics.push(make_diag(ctx, start_line));
        }
        diagnostics
    }
}

fn make_diag(ctx: &CheckCtx, line: usize) -> Diagnostic {
    Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line,
        column: 1,
        rule_id: super::META.id.into(),
        message: "`is_deleted BOOLEAN` loses the deletion time — use `deleted_at TIMESTAMPTZ NULL` instead.".into(),
        severity: Severity::Warning,
        span: None,
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
    fn flags_is_deleted_boolean_in_create_table() {
        let src = "CREATE TABLE users (id UUID, is_deleted BOOLEAN NOT NULL);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_is_deleted_in_alter_table() {
        let src = "ALTER TABLE users ADD COLUMN is_deleted BOOLEAN NOT NULL;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_dml() {
        let src = "SELECT * FROM users WHERE is_deleted = false;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_deleted_at_timestamp() {
        let src = "CREATE TABLE users (id UUID, deleted_at TIMESTAMPTZ);";
        assert!(run(src).is_empty());
    }
}
