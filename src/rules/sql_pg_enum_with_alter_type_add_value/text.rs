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
            let trimmed = upper.trim();

            if trimmed == "BEGIN"
                || trimmed == "BEGIN;"
                || trimmed.starts_with("BEGIN ")
                || trimmed.starts_with("START TRANSACTION")
            {
                in_tx = true;
            }

            if in_tx && upper.contains("ALTER TYPE") && upper.contains("ADD VALUE") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-pg-enum-with-alter-type-add-value".into(),
                    message: "`ALTER TYPE ... ADD VALUE` cannot run inside a transaction block. Run it outside `BEGIN`/`COMMIT`, or use a CHECK-constrained text column instead of an ENUM.".into(),
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
    fn flags_add_value_inside_begin() {
        let src = "BEGIN;\nALTER TYPE status ADD VALUE 'archived';\nCOMMIT;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_add_value_outside_tx() {
        let src = "ALTER TYPE status ADD VALUE 'archived';";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_add_value_after_commit() {
        let src = "BEGIN;\nUPDATE t SET x = 1;\nCOMMIT;\nALTER TYPE status ADD VALUE 'archived';";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_create_type_in_tx() {
        // CREATE TYPE is allowed in a transaction.
        let src = "BEGIN;\nCREATE TYPE status AS ENUM ('a', 'b');\nCOMMIT;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_inside_start_transaction() {
        let src = "START TRANSACTION;\nALTER TYPE s ADD VALUE 'x';\nCOMMIT;";
        assert_eq!(run(src).len(), 1);
    }
}
