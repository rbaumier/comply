use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let upper_source = ctx.source.to_ascii_uppercase();
        let lines: Vec<&str> = upper_source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if !line.contains("ADD CONSTRAINT") {
                continue;
            }
            // ADD CONSTRAINT detected — verify NOT VALID appears on same or
            // following line (statement may span multiple lines).
            let window_end = (idx + 5).min(lines.len());
            let has_not_valid = (idx..window_end).any(|k| lines[k].contains("NOT VALID"));
            // Ignore CHECK-less benign constraints like UNIQUE or PRIMARY KEY
            // which don't require a table scan when created NOT VALID anyway.
            // We focus on the common case: CHECK or FOREIGN KEY.
            let constraint_window: String = (idx..window_end).map(|k| lines[k]).collect::<Vec<_>>().join(" ");
            let is_scan_heavy = constraint_window.contains("CHECK")
                || constraint_window.contains("FOREIGN KEY");
            if is_scan_heavy && !has_not_valid {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "ADD CONSTRAINT without NOT VALID locks the table during the scan — split into ADD ... NOT VALID + VALIDATE CONSTRAINT.".into(),
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
    fn flags_add_check_without_not_valid() {
        assert_eq!(
            run("`ALTER TABLE t ADD CONSTRAINT t_age_chk CHECK (age > 0);`").len(),
            1
        );
    }

    #[test]
    fn flags_add_fk_without_not_valid() {
        assert_eq!(
            run("`ALTER TABLE t ADD CONSTRAINT t_u_fk FOREIGN KEY (u) REFERENCES user(id);`").len(),
            1
        );
    }

    #[test]
    fn allows_not_valid() {
        assert!(run(
            "`ALTER TABLE t ADD CONSTRAINT t_age_chk CHECK (age > 0) NOT VALID;`"
        )
        .is_empty());
    }
}
