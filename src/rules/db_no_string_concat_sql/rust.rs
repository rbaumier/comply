//! db-no-string-concat-sql Rust backend.
//!
//! Flag `format!("SELECT ... {}", var)` — string interpolation with SQL keywords.

use crate::diagnostic::{Diagnostic, Severity};

const SQL_KEYWORDS: &[&str] = &["SELECT", "INSERT", "UPDATE", "DELETE", "WHERE", "FROM"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "macro_invocation" {
        return;
    }

    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if mac_name != "format" && mac_name != "format_args" {
        return;
    }

    let Ok(full_text) = node.utf8_text(source) else { return };
    let upper = full_text.to_ascii_uppercase();

    let has_sql = SQL_KEYWORDS.iter().any(|k| upper.contains(k));
    if !has_sql {
        return;
    }

    // Must have interpolation (curly braces with content).
    if !full_text.contains('{') || full_text.matches("{}").count() + full_text.matches("{:").count() == 0 {
        // No interpolation — just a static string with SQL keywords.
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "db-no-string-concat-sql".into(),
        message: "String interpolation with SQL keywords — use parameterized queries.".into(),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_format_with_sql() {
        let src = r#"fn f(id: i32) { let q = format!("SELECT * FROM users WHERE id = {}", id); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_static_sql() {
        let src = r#"fn f() { let q = "SELECT * FROM users"; }"#;
        assert!(run_on(src).is_empty());
    }
}
