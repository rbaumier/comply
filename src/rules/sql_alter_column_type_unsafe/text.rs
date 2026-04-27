use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let upper = ctx.source.to_ascii_uppercase();

        // Walk statement-by-statement (split on `;`) and check each statement
        // for "ALTER COLUMN ... TYPE" without "USING" in the same statement.
        let mut stmt_start_byte = 0usize;
        for (i, ch) in upper.char_indices() {
            if ch != ';' {
                continue;
            }
            let stmt = &upper[stmt_start_byte..i];
            if statement_is_alter_type_without_using(stmt) {
                let line = upper[..stmt_start_byte].matches('\n').count() + 1;
                // Refine to the line that actually contains "TYPE".
                let line = stmt
                    .find("TYPE")
                    .map(|off| line + upper[stmt_start_byte..stmt_start_byte + off].matches('\n').count())
                    .unwrap_or(line);
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column: 1,
                    rule_id: "sql-alter-column-type-unsafe".into(),
                    message: "`ALTER COLUMN ... TYPE` without a `USING` clause may rewrite the entire table. Add a `USING` cast or use expand/contract.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            stmt_start_byte = i + 1;
        }
        diagnostics
    }
}

fn statement_is_alter_type_without_using(stmt: &str) -> bool {
    if !stmt.contains("ALTER TABLE") {
        return false;
    }
    if !stmt.contains("ALTER COLUMN") {
        return false;
    }
    // Look for "TYPE" as a keyword after ALTER COLUMN — accept "TYPE " or "SET DATA TYPE ".
    let has_type = stmt.contains(" TYPE ") || stmt.contains("\nTYPE ") || stmt.contains("SET DATA TYPE");
    if !has_type {
        return false;
    }
    !stmt.contains("USING ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), src))
    }

    #[test]
    fn flags_alter_type_without_using() {
        assert_eq!(
            run("ALTER TABLE users ALTER COLUMN age TYPE BIGINT;").len(),
            1
        );
    }

    #[test]
    fn allows_alter_type_with_using() {
        assert!(
            run("ALTER TABLE users ALTER COLUMN age TYPE BIGINT USING age::BIGINT;").is_empty()
        );
    }

    #[test]
    fn flags_set_data_type_without_using() {
        assert_eq!(
            run("ALTER TABLE users ALTER COLUMN age SET DATA TYPE BIGINT;").len(),
            1
        );
    }

    #[test]
    fn allows_create_table() {
        // No ALTER TABLE — should not match.
        assert!(run("CREATE TABLE users (age INT);").is_empty());
    }

    #[test]
    fn flags_lowercase() {
        assert_eq!(
            run("alter table users alter column age type bigint;").len(),
            1
        );
    }
}
