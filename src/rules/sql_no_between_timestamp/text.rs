use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TIMESTAMP_HINTS: &[&str] = &["timestamp", "created_at", "updated_at", "date", "time", "_at "];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            if lower.contains("between") && TIMESTAMP_HINTS.iter().any(|h| lower.contains(h)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1, column: 1,
                    rule_id: "sql-no-between-timestamp".into(),
                    message: "`BETWEEN` with timestamps is inclusive both sides — causes off-by-one. Use `>= start AND < end` instead.".into(),
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
    fn run(source: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), source)) }

    #[test]
    fn flags() { assert_eq!(run("WHERE created_at BETWEEN '2024-01-01' AND '2024-12-31'").len(), 1); }
    #[test]
    fn allows_range() { assert!(run("WHERE created_at >= '2024-01-01' AND created_at < '2025-01-01'").is_empty()); }
}
