use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let line_upper = line.to_ascii_uppercase();
            if line_upper.contains("SELECT *") || line_upper.contains("SELECT  *") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-no-select-star".into(),
                    message: "`SELECT *` wastes bandwidth — list columns explicitly so the API contract is visible and covering indexes can work.".into(),
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
        assert_eq!(run("const q = `SELECT * FROM users`;").len(), 1);
    }

    #[test]
    fn flags_lowercase() {
        assert_eq!(run("const q = \"select * from users\";").len(), 1);
    }

    #[test]
    fn allows_explicit() {
        assert!(run("const q = `SELECT id, name FROM users`;").is_empty());
    }
}
