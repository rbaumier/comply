use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            if !upper.contains("DROP TABLE") {
                continue;
            }
            let has_cascade = upper.contains("CASCADE");
            let has_if_exists = upper.contains("IF EXISTS");
            if has_cascade {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-drop-table-no-cascade-warning".into(),
                    message: "`DROP TABLE ... CASCADE` silently removes views, foreign keys, and other dependents. Drop them explicitly so the migration is auditable.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            } else if !has_if_exists {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-drop-table-no-cascade-warning".into(),
                    message: "`DROP TABLE` without `IF EXISTS` errors on rerun. Add `IF EXISTS` so the migration is idempotent.".into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), src))
    }

    #[test]
    fn flags_drop_table_without_if_exists() {
        assert_eq!(run("DROP TABLE users;").len(), 1);
    }

    #[test]
    fn flags_drop_table_cascade() {
        assert_eq!(run("DROP TABLE IF EXISTS users CASCADE;").len(), 1);
    }

    #[test]
    fn allows_drop_table_if_exists() {
        assert!(run("DROP TABLE IF EXISTS users;").is_empty());
    }

    #[test]
    fn allows_unrelated_statement() {
        assert!(run("CREATE TABLE users (id INT);").is_empty());
    }

    #[test]
    fn flags_lowercase_cascade() {
        assert_eq!(run("drop table if exists users cascade;").len(), 1);
    }
}
