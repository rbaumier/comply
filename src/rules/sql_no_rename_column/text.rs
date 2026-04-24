use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            if upper.contains("RENAME COLUMN") && upper.contains("ALTER TABLE") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "RENAME COLUMN breaks in-flight queries — use expand-contract (add, dual-write, backfill, drop).".into(),
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
    fn flags_rename_column() {
        assert_eq!(
            run("`ALTER TABLE account RENAME COLUMN email TO email_address;`").len(),
            1
        );
    }

    #[test]
    fn allows_add_column() {
        assert!(
            run("`ALTER TABLE account ADD COLUMN email_address TEXT;`").is_empty()
        );
    }
}
