use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const DDL_STARTS: &[&str] = &["ALTER TABLE", "CREATE INDEX", "DROP INDEX", "ADD CONSTRAINT"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let upper_source = ctx.source.to_ascii_uppercase();
        if upper_source.contains("LOCK_TIMEOUT") {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.trim().to_ascii_uppercase();
            if DDL_STARTS.iter().any(|d| upper.starts_with(d)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(), line: idx + 1, column: 1,
                    rule_id: "migration-needs-lock-timeout".into(),
                    message: "DDL without `SET lock_timeout` — add `SET lock_timeout = '5s';` at the top to prevent write queue pileups.".into(),
                    severity: Severity::Warning,
                });
                break; // One diagnostic per file is enough.
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.sql"), source)) }

    #[test]
    fn flags() { assert_eq!(run("ALTER TABLE users ADD COLUMN age INT;").len(), 1); }
    #[test]
    fn allows() { assert!(run("SET lock_timeout = '5s';\nALTER TABLE users ADD COLUMN age INT;").is_empty()); }
}
