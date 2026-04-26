//! sql-no-varchar — `.sql` file backend.
//!
//! In a `.sql` file the whole content is SQL, so there's no string
//! literal to filter. We scan line by line for `VARCHAR(` / `CHAR(`
//! using the same word-boundary helper as the AST backends to avoid
//! matching identifiers that happen to end in `_char`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !super::sql_uses_varchar_or_char(line) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: "sql-no-varchar".into(),
                message: "`VARCHAR(N)` / `CHAR(N)` provides no perf benefit \
                          in PostgreSQL — use `TEXT` with a CHECK constraint."
                    .into(),
                severity: Severity::Error,
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
    fn flags_varchar_in_sql_file() {
        assert_eq!(
            run("CREATE TABLE users (\n  name VARCHAR(255) NOT NULL\n);").len(),
            1
        );
    }

    #[test]
    fn does_not_flag_text_column() {
        assert!(run("CREATE TABLE users (\n  name TEXT NOT NULL\n);").is_empty());
    }
}
