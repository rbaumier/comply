use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            if upper.contains("TRUNCATE TABLE") || upper.contains("TRUNCATE ") {
                // Avoid false positives on words containing "truncate" elsewhere
                let before_idx = upper.find("TRUNCATE").unwrap();
                let before_char = line[..before_idx].chars().last();
                if before_char.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`TRUNCATE` bypasses triggers and audit — use `DELETE FROM` instead.".into(),
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
    fn flags_truncate_table() {
        assert_eq!(run("`TRUNCATE TABLE users`").len(), 1);
    }

    #[test]
    fn flags_truncate_bare() {
        assert_eq!(run("`TRUNCATE users`").len(), 1);
    }

    #[test]
    fn allows_delete_from() {
        assert!(run("`DELETE FROM users`").is_empty());
    }
}
