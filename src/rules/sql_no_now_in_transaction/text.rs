use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let upper_source = ctx.source.to_ascii_uppercase();
        // Find BEGIN ... COMMIT/END blocks and flag NOW() inside.
        let mut in_tx = false;
        for (idx, line) in upper_source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("BEGIN;")
                || trimmed == "BEGIN"
                || trimmed.starts_with("BEGIN ")
                || trimmed.contains("START TRANSACTION")
            {
                in_tx = true;
                continue;
            }
            if trimmed.starts_with("COMMIT") || trimmed.starts_with("ROLLBACK") || trimmed == "END;"
            {
                in_tx = false;
                continue;
            }
            if !in_tx {
                continue;
            }
            if line.contains("NOW()") || line.contains("CURRENT_TIMESTAMP") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`NOW()`/`CURRENT_TIMESTAMP` freezes at transaction start — use `clock_timestamp()` inside BEGIN blocks.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_now_in_begin_block() {
        let src = "BEGIN;\nINSERT INTO log (ts) VALUES (NOW());\nCOMMIT;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_now_outside_tx() {
        assert!(run("INSERT INTO log (ts) VALUES (NOW());").is_empty());
    }

    #[test]
    fn allows_clock_timestamp_in_tx() {
        let src = "BEGIN;\nINSERT INTO log (ts) VALUES (clock_timestamp());\nCOMMIT;";
        assert!(run(src).is_empty());
    }
}
