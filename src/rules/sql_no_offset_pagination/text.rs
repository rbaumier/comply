use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
    let line_upper = line.to_ascii_uppercase();
            if line_upper.contains("OFFSET") && line_upper.contains("LIMIT") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-no-offset-pagination".into(),
                    message: "`OFFSET` pagination is O(N) on deep pages — use cursor-based (keyset) pagination: `WHERE id > :last_id ORDER BY id LIMIT N`.".into(),
                    severity: Severity::Warning,
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
        assert_eq!(run("const q = `SELECT id FROM t LIMIT 10 OFFSET 100`;").len(), 1);
    }

    #[test]
    fn allows_no_offset() {
        assert!(run("const q = `SELECT id FROM t LIMIT 10`;").is_empty());
    }
}
