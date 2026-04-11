//! no-sql-string-format backend — SQL with string interpolation.

use crate::diagnostic::{Diagnostic, Severity};

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
    // String concatenation
    if (line.contains('"') || line.contains('\'')) && line.contains(" + ") {
        return true;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");

    for (idx, line) in text.lines().enumerate() {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_template_literal_select() {
        assert_eq!(run_on("const q = `SELECT * FROM users WHERE id = ${userId}`;").len(), 1);
    }

    #[test]
    fn flags_string_concat() {
        assert_eq!(run_on(r#"const q = "SELECT * FROM users WHERE id = " + userId;"#).len(), 1);
    }

    #[test]
    fn allows_parameterized_query() {
        assert!(run_on(r#"db.query("SELECT * FROM users WHERE id = ?", [userId]);"#).is_empty());
    }

    #[test]
    fn allows_template_without_interpolation() {
        assert!(run_on("const q = `SELECT * FROM users`;").is_empty());
    }
}
