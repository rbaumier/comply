use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut in_tx = false;

        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();

            // Track BEGIN ... COMMIT (or ROLLBACK) blocks. BEGIN is a whole-line
            // keyword in migrations; allow trailing semicolon.
            let trimmed = upper.trim();
            if trimmed == "BEGIN"
                || trimmed == "BEGIN;"
                || trimmed.starts_with("BEGIN ")
                || trimmed.starts_with("START TRANSACTION")
            {
                in_tx = true;
            }

            if in_tx && upper.contains("CREATE INDEX") && upper.contains("CONCURRENTLY") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-create-index-in-transaction".into(),
                    message: "`CREATE INDEX CONCURRENTLY` cannot run inside a transaction block. Move it outside `BEGIN`/`COMMIT`.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }

            if trimmed == "COMMIT"
                || trimmed == "COMMIT;"
                || trimmed == "ROLLBACK"
                || trimmed == "ROLLBACK;"
            {
                in_tx = false;
            }
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
    fn flags_concurrently_inside_begin() {
        let src = "BEGIN;\nCREATE INDEX CONCURRENTLY idx ON users(email);\nCOMMIT;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_concurrently_outside_tx() {
        let src = "CREATE INDEX CONCURRENTLY idx ON users(email);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_concurrently_after_commit() {
        let src = "BEGIN;\nALTER TABLE users ADD c INT;\nCOMMIT;\nCREATE INDEX CONCURRENTLY idx ON users(c);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_concurrently_inside_start_transaction() {
        let src = "START TRANSACTION;\nCREATE INDEX CONCURRENTLY idx ON t(c);\nCOMMIT;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_create_index_without_concurrently_in_tx() {
        // Plain CREATE INDEX is allowed in a transaction (a different rule
        // flags the missing CONCURRENTLY).
        let src = "BEGIN;\nCREATE INDEX idx ON users(email);\nCOMMIT;";
        assert!(run(src).is_empty());
    }
}
