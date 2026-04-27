use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let upper_lines: Vec<String> = ctx.source.lines().map(|l| l.to_ascii_uppercase()).collect();

        // Track whether we're inside an ALTER TABLE statement: SET NOT NULL
        // can appear on a different line than ALTER TABLE.
        let mut in_alter_table = false;
        for (idx, upper) in upper_lines.iter().enumerate() {
            if upper.contains("ALTER TABLE") {
                in_alter_table = true;
            }
            if in_alter_table && upper.contains("SET NOT NULL") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-add-not-null-without-default".into(),
                    message: "`SET NOT NULL` on an existing table requires a full scan under ACCESS EXCLUSIVE lock. Use `CHECK (... IS NOT NULL) NOT VALID` then `VALIDATE CONSTRAINT` instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            if upper.contains(';') {
                in_alter_table = false;
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
    fn flags_set_not_null_single_line() {
        assert_eq!(
            run("ALTER TABLE users ALTER COLUMN email SET NOT NULL;").len(),
            1
        );
    }

    #[test]
    fn flags_set_not_null_multi_line() {
        let src = "ALTER TABLE users\n    ALTER COLUMN email SET NOT NULL;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_check_not_valid() {
        assert!(
            run("ALTER TABLE users ADD CONSTRAINT chk CHECK (email IS NOT NULL) NOT VALID;")
                .is_empty()
        );
    }

    #[test]
    fn allows_create_table_not_null() {
        // CREATE TABLE NOT NULL is fine — only existing-table ALTER is dangerous.
        assert!(run("CREATE TABLE users (email TEXT NOT NULL);").is_empty());
    }

    #[test]
    fn flags_lowercase() {
        assert_eq!(
            run("alter table users alter column email set not null;").len(),
            1
        );
    }
}
