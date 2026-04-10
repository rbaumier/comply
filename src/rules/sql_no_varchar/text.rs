use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
    let line_upper = line.to_ascii_uppercase();
            if line_upper.contains("VARCHAR(") || line_upper.contains("CHAR(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-no-varchar".into(),
                    message: "`VARCHAR(N)` / `CHAR(N)` provides no perf benefit in PostgreSQL — use `TEXT` with `CHECK(length(col) <= N)`.".into(),
                    severity: Severity::Error,
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
    fn flags_varchar() {
        assert_eq!(run("name VARCHAR(255) NOT NULL").len(), 1);
    }

    #[test]
    fn flags_char() {
        assert_eq!(run("code CHAR(3)").len(), 1);
    }

    #[test]
    fn allows_text() {
        assert!(run("name TEXT NOT NULL").is_empty());
    }
}
