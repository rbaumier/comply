use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("LIKE '%")
                || line.contains("like '%")
                || line.contains("LIKE \"%")
                || line.contains("like \"%")
            {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-no-like-wildcard-prefix".into(),
                    message: "`LIKE '%...'` forces a sequential scan — use TSVECTOR + GIN index with `@@` for full-text search.".into(),
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
        assert_eq!(run("WHERE name LIKE '%test%'").len(), 1);
    }

    #[test]
    fn allows_suffix() {
        assert!(run("WHERE name LIKE 'test%'").is_empty());
    }
}
