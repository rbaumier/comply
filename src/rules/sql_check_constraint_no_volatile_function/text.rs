use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const VOLATILE: &[&str] = &[
    "NOW(",
    "CURRENT_TIMESTAMP",
    "CURRENT_DATE",
    "CURRENT_TIME",
    "LOCALTIMESTAMP",
    "LOCALTIME",
    "RANDOM(",
    "CLOCK_TIMESTAMP(",
    "STATEMENT_TIMESTAMP(",
    "TIMEOFDAY(",
];

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["CHECK"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            if !upper.contains("CHECK") || !upper.contains('(') {
                continue;
            }
            // Extract content between the first CHECK( and the matching close, on this line.
            let Some(check_idx) = upper.find("CHECK") else {
                continue;
            };
            let after_check = &upper[check_idx + 5..];
            let Some(open) = after_check.find('(') else {
                continue;
            };
            let body = &after_check[open..];
            if VOLATILE.iter().any(|v| body.contains(v)) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-check-constraint-no-volatile-function".into(),
                    message: "`CHECK` constraint contains a volatile function (`NOW()`, `random()`, …). CHECK must be deterministic — move this to a trigger.".into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), src))
    }

    #[test]
    fn flags_now_in_check() {
        assert_eq!(
            run("CREATE TABLE t (created_at TIMESTAMPTZ CHECK (created_at <= NOW()));").len(),
            1
        );
    }

    #[test]
    fn flags_current_timestamp_in_check() {
        assert_eq!(
            run("ALTER TABLE t ADD CONSTRAINT chk CHECK (ts <= CURRENT_TIMESTAMP);").len(),
            1
        );
    }

    #[test]
    fn flags_random_in_check() {
        assert_eq!(
            run("CREATE TABLE t (val NUMERIC CHECK (val > random()));").len(),
            1
        );
    }

    #[test]
    fn allows_static_check() {
        assert!(run("CREATE TABLE t (age INT CHECK (age >= 0));").is_empty());
    }

    #[test]
    fn allows_now_outside_check() {
        // NOW() in DEFAULT is allowed.
        assert!(run("CREATE TABLE t (created_at TIMESTAMPTZ DEFAULT NOW());").is_empty());
    }
}
