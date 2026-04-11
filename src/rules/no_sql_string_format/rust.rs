//! no-sql-string-format backend for Rust.
//!
//! Flags `format!("SELECT ... {}", val)` and similar SQL string interpolation
//! patterns. Use parameterized queries instead.

use crate::diagnostic::{Diagnostic, Severity};

const SQL_KEYWORDS: &[&str] = &["SELECT", "INSERT", "UPDATE", "DELETE", "WHERE"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "macro_invocation" {
        return;
    }

    // Check if the macro is `format!`
    let Some(mac) = node.child_by_field_name("macro") else { return };
    let mac_name = mac.utf8_text(source).unwrap_or("");
    if mac_name != "format" && mac_name != "format_args" {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    let upper = text.to_uppercase();

    // Check for SQL keywords in the format string
    let has_sql = SQL_KEYWORDS.iter().any(|kw| upper.contains(kw));
    if !has_sql {
        return;
    }

    // Check for interpolation (`{}`, `{name}`, `{0}`)
    if text.contains('{') && text.contains('}') {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-sql-string-format".into(),
            message: "SQL query built with `format!` — use parameterized queries to prevent injection.".into(),
            severity: Severity::Error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_format_select() {
        assert_eq!(
            run_on(r#"fn f(id: &str) { let q = format!("SELECT * FROM users WHERE id = {}", id); }"#).len(),
            1,
        );
    }

    #[test]
    fn flags_format_insert() {
        assert_eq!(
            run_on(r#"fn f(v: &str) { let q = format!("INSERT INTO t VALUES ({})", v); }"#).len(),
            1,
        );
    }

    #[test]
    fn allows_format_without_sql() {
        assert!(run_on(r#"fn f() { let s = format!("hello {}", name); }"#).is_empty());
    }

    #[test]
    fn allows_sql_without_interpolation() {
        assert!(run_on(r#"fn f() { let q = "SELECT * FROM users"; }"#).is_empty());
    }
}
