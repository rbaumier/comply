use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            // Match `autovacuum_enabled = false` or `autovacuum_enabled=false`
            // with optional whitespace.
            let compact: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
            if compact.contains("autovacuum_enabled=false")
                || compact.contains("autovacuum_enabled=off")
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Disabling autovacuum causes bloat and XID wraparound — tune thresholds instead.".into(),
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
    fn flags_disable_autovacuum() {
        assert_eq!(
            run("`ALTER TABLE t SET (autovacuum_enabled = false)`").len(),
            1
        );
    }

    #[test]
    fn flags_off_variant() {
        assert_eq!(
            run("`ALTER TABLE t SET (autovacuum_enabled = off)`").len(),
            1
        );
    }

    #[test]
    fn allows_threshold_tuning() {
        assert!(
            run("`ALTER TABLE t SET (autovacuum_vacuum_scale_factor = 0.01)`").is_empty()
        );
    }
}
