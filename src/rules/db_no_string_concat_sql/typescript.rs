//! db-no-string-concat-sql — flag string concatenation with SQL keywords.
//!
//! Looks for binary_expression nodes with `+` operator where one side
//! contains SQL keywords and the other side is a non-literal expression,
//! indicating potential SQL injection.

use crate::diagnostic::{Diagnostic, Severity};

const SQL_KEYWORDS: &[&str] = &["SELECT", "INSERT", "UPDATE", "DELETE", "WHERE", "FROM"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "binary_expression" {
        return;
    }

    let Some(op_node) = node.child_by_field_name("operator") else { return };
    if op_node.utf8_text(source).unwrap_or("") != "+" {
        return;
    }

    // Collect the full text of the binary expression to check for SQL keywords.
    let text = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };

    let upper = text.to_ascii_uppercase();
    let has_sql = SQL_KEYWORDS.iter().any(|k| upper.contains(k));
    if !has_sql {
        return;
    }

    // Make sure at least one side is a string literal (so this is string concat).
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let left_is_string = is_string_node(&left);
    let right_is_string = is_string_node(&right);

    // At least one side must be a string, and the other side must be a
    // non-literal (variable / expression) for this to be dangerous.
    if !left_is_string && !right_is_string {
        return;
    }

    // Skip if this looks like parameterized ($1, $2)
    if text.contains("$1") || text.contains("$2") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "db-no-string-concat-sql".into(),
        message: "String concatenation with SQL keywords \
                  — SQL injection risk. Use parameterized queries \
                  (`$1`, `?`) instead."
            .into(),
        severity: Severity::Error,
    });
}

fn is_string_node(node: &tree_sitter::Node) -> bool {
    matches!(node.kind(), "string" | "template_string")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_concat() {
        assert_eq!(
            run_on(r#"const q = "SELECT * FROM users WHERE id = " + userId;"#).len(),
            1
        );
    }

    #[test]
    fn allows_param() {
        assert!(
            run_on(r#"const q = "SELECT * FROM users WHERE id = $1";"#).is_empty()
        );
    }

    #[test]
    fn allows_no_sql() {
        assert!(run_on(r#"const msg = "hello " + name;"#).is_empty());
    }
}
