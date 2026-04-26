//! sql-no-between-timestamp — `.sql` file backend.
//!
//! For pure SQL files there is no string-literal layer to filter — the
//! whole file is SQL — so we apply the same `sql_uses_between_on_timestamp`
//! heuristic against the raw content. To avoid scanning the entire file
//! as one buffer (and to give per-line diagnostic positions), we run the
//! heuristic line by line.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !super::sql_uses_between_on_timestamp(line) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: "sql-no-between-timestamp".into(),
                message: "`BETWEEN` with timestamps is inclusive on both \
                          sides — use `>= start AND < end` instead."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
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
    fn flags_in_sql_file() {
        let src = "SELECT * FROM events WHERE created_at BETWEEN '2024-01-01' AND '2024-12-31';";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_between_on_id() {
        let src = "SELECT * FROM users WHERE id BETWEEN 1 AND 100;";
        assert!(run(src).is_empty());
    }
}
