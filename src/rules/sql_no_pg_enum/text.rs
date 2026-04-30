use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let line_upper = line.to_ascii_uppercase();
            if line_upper.contains("AS ENUM") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-no-pg-enum".into(),
                    message: "PostgreSQL `CREATE TYPE ... AS ENUM` is append-only — you can\'t remove values. Use `TEXT CHECK(col IN (...))` or a lookup table instead.".into(),
                    severity: Severity::Error,
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
        assert_eq!(
            run("CREATE TYPE status AS ENUM ('active', 'inactive');").len(),
            1
        );
    }

    #[test]
    fn allows_check() {
        assert!(run("status TEXT CHECK(status IN ('active', 'inactive'))").is_empty());
    }
}
