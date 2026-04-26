//! sql-no-offset-pagination — `.sql` file backend.
//!
//! Pure SQL files are entirely SQL, so there's no string-literal layer
//! to filter. We split the file into statements (`;`-terminated chunks)
//! and run the same `sql_uses_offset_pagination` heuristic on each
//! statement, since the rule looks for the co-occurrence of `LIMIT` and
//! `OFFSET` within a single query.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

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
                if super::sql_uses_offset_pagination(&buf) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: start_line,
                        column: 1,
                        rule_id: "sql-no-offset-pagination".into(),
                        message: "`OFFSET` pagination is O(N) on deep pages — use \
                                  cursor-based pagination: \
                                  `WHERE id > :last_id ORDER BY id LIMIT N`."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                buf.clear();
            }
        }
        // Trailing statement without semicolon.
        if !buf.trim().is_empty() && super::sql_uses_offset_pagination(&buf) {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: start_line,
                column: 1,
                rule_id: "sql-no-offset-pagination".into(),
                message: "`OFFSET` pagination is O(N) on deep pages — use \
                          cursor-based pagination: \
                          `WHERE id > :last_id ORDER BY id LIMIT N`."
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
        assert_eq!(run("SELECT * FROM users LIMIT 10 OFFSET 100;").len(), 1);
    }

    #[test]
    fn does_not_flag_limit_only() {
        assert!(run("SELECT * FROM users LIMIT 10;").is_empty());
    }
}
