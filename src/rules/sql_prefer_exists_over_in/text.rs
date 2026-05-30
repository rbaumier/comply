use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if super::is_test_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let line_upper = line.to_ascii_uppercase();
            if line_upper.contains("IN (SELECT") || line_upper.contains("IN(SELECT") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-prefer-exists-over-in".into(),
                    message: "`IN (SELECT ...)` materializes the entire subquery — use `EXISTS (SELECT 1 ...)` which short-circuits on first match.".into(),
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
    fn flags() {
        assert_eq!(run("WHERE id IN (SELECT user_id FROM orders)").len(), 1);
    }

    #[test]
    fn allows_exists() {
        assert!(run("WHERE EXISTS (SELECT 1 FROM orders WHERE orders.user_id = u.id)").is_empty());
    }

    #[test]
    fn allows_in_list() {
        assert!(run("WHERE id IN (1, 2, 3)").is_empty());
    }

    #[test]
    fn no_fp_in_test_file() {
        // Regression for #528.
        let src = "DELETE FROM users WHERE id IN (SELECT id FROM temp)";
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("cleanup.integration.test.ts"),
            src,
        ));
        assert!(diags.is_empty());
    }
}
