//! strings-comparison Rust backend — flag relational operators with
//! string literals.

use crate::diagnostic::{Diagnostic, Severity};

const RELATIONAL_OPS: &[&str] = &["<", ">", "<=", ">="];

fn is_string_node(node: tree_sitter::Node) -> bool {
    matches!(node.kind(), "string_literal" | "raw_string_literal")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "binary_expression" {
        return;
    }

    // Check operator.
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let Ok(op) = op_node.utf8_text(source) else { return };
    if !RELATIONAL_OPS.contains(&op) {
        return;
    }

    // Check if either operand is a string literal.
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    if !is_string_node(left) && !is_string_node(right) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "strings-comparison".into(),
        message: "Relational comparison with string literal uses lexicographic order \u{2014} this is rarely the intent.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_string_less_than() {
        assert_eq!(run_on(r#"fn f() { let _ = "abc" < "def"; }"#).len(), 1);
    }

    #[test]
    fn flags_var_greater_than_string() {
        assert_eq!(run_on(r#"fn f(name: &str) { let _ = name > "xyz"; }"#).len(), 1);
    }

    #[test]
    fn allows_equality_comparison() {
        assert!(run_on(r#"fn f(x: &str) { let _ = x == "hello"; }"#).is_empty());
    }

    #[test]
    fn allows_number_comparison() {
        assert!(run_on("fn f(x: i32) { let _ = x > 5; }").is_empty());
    }
}
