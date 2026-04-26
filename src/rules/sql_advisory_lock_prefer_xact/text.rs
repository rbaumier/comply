use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if !line.contains("pg_advisory_lock(") {
                continue;
            }
            if line.contains("pg_advisory_xact_lock(") || line.contains("pg_try_advisory") {
                continue;
            }
            if let Some(col) = line.find("pg_advisory_lock(") {
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: i + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "Use `pg_advisory_xact_lock()` instead of `pg_advisory_lock()` — it releases automatically at transaction end.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_session_lock() {
        assert_eq!(run("SELECT pg_advisory_lock(123);").len(), 1);
    }

    #[test]
    fn allows_xact_lock() {
        assert!(run("SELECT pg_advisory_xact_lock(123);").is_empty());
    }

    #[test]
    fn allows_try_lock() {
        assert!(run("SELECT pg_try_advisory_lock(123);").is_empty());
    }
}
