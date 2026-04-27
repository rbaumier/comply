use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let upper = ctx.source.to_ascii_uppercase();

        // Walk statement-by-statement.
        let mut stmt_start = 0usize;
        for (i, ch) in upper.char_indices() {
            if ch != ';' {
                continue;
            }
            let stmt = &upper[stmt_start..i];
            if stmt.contains("FOR UPDATE")
                && !stmt.contains("SKIP LOCKED")
                && !stmt.contains("NOWAIT")
            {
                let off = stmt.find("FOR UPDATE").unwrap_or(0);
                let line = upper[..stmt_start + off].matches('\n').count() + 1;
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column: 1,
                    rule_id: "sql-no-for-update-without-skip-locked".into(),
                    message: "`SELECT FOR UPDATE` without `SKIP LOCKED` or `NOWAIT` serializes every worker. Add `SKIP LOCKED` for queues or `NOWAIT` to fail fast.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            stmt_start = i + 1;
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
    fn flags_for_update_alone() {
        assert_eq!(run("SELECT * FROM jobs FOR UPDATE;").len(), 1);
    }

    #[test]
    fn allows_for_update_skip_locked() {
        assert!(run("SELECT * FROM jobs FOR UPDATE SKIP LOCKED;").is_empty());
    }

    #[test]
    fn allows_for_update_nowait() {
        assert!(run("SELECT * FROM jobs FOR UPDATE NOWAIT;").is_empty());
    }

    #[test]
    fn allows_plain_select() {
        assert!(run("SELECT * FROM jobs;").is_empty());
    }

    #[test]
    fn flags_lowercase() {
        assert_eq!(run("select * from jobs for update;").len(), 1);
    }
}
