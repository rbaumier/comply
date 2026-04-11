use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const SQL_KEYWORDS: &[&str] = &["SELECT", "INSERT", "UPDATE", "DELETE", "WHERE"];

/// Returns true if the line contains a SQL keyword inside a template literal
/// with interpolation (`${`), or SQL keyword concatenated with `+`.
fn has_sql_interpolation(line: &str) -> bool {
    let upper = line.to_uppercase();
    let has_sql_keyword = SQL_KEYWORDS.iter().any(|kw| upper.contains(kw));
    if !has_sql_keyword {
        return false;
    }
    // Template literal with interpolation
    if line.contains('`') && line.contains("${") {
        return true;
    }
    // String concatenation: quote + variable via +
    // e.g. "SELECT * FROM users WHERE id = " + userId
    if (line.contains('"') || line.contains('\'')) && line.contains(" + ") {
        return true;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_sql_interpolation(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-sql-string-format".into(),
                    message: "SQL query built with string interpolation — use parameterized queries to prevent injection.".into(),
                    severity: Severity::Error,
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
    fn flags_template_literal_select() {
        assert_eq!(run("const q = `SELECT * FROM users WHERE id = ${userId}`;").len(), 1);
    }

    #[test]
    fn flags_template_literal_insert() {
        assert_eq!(run("db.query(`INSERT INTO logs VALUES (${val})`);").len(), 1);
    }

    #[test]
    fn flags_string_concat() {
        assert_eq!(run(r#"const q = "SELECT * FROM users WHERE id = " + userId;"#).len(), 1);
    }

    #[test]
    fn allows_parameterized_query() {
        assert!(run(r#"db.query("SELECT * FROM users WHERE id = ?", [userId]);"#).is_empty());
    }

    #[test]
    fn allows_template_without_interpolation() {
        assert!(run("const q = `SELECT * FROM users`;").is_empty());
    }
}
