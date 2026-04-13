//! ts-no-dynamic-delete backend — flag `delete obj[expr]` where `expr` is
//! not a literal string/number.
//!
//! Detection: walk `unary_expression` nodes with operator `delete`, check
//! if the argument is a `subscript_expression` with a non-literal index.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "unary_expression" {
        return;
    }
    // Check operator is "delete"
    let Some(op_node) = node.child_by_field_name("operator") else {
        return;
    };
    let op_text = &source[op_node.byte_range()];
    if op_text != b"delete" {
        return;
    }
    let Some(arg) = node.child_by_field_name("argument") else {
        return;
    };
    // Must be a subscript (computed) access: obj[expr]
    if arg.kind() != "subscript_expression" {
        return;
    }
    let Some(index) = arg.child_by_field_name("index") else {
        return;
    };
    // Allow literal string/number keys and negative numeric literals
    let index_kind = index.kind();
    if index_kind == "string" || index_kind == "number" {
        return;
    }
    // Allow negative number: unary_expression with `-` and number operand
    if index_kind == "unary_expression" {
        let idx_text = &source[index.byte_range()];
        if let Ok(s) = std::str::from_utf8(idx_text) {
            let s = s.trim();
            if s.starts_with('-') && s[1..].trim().parse::<f64>().is_ok() {
                return;
            }
        }
    }
    let pos = index.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-dynamic-delete".into(),
        message: "Do not delete dynamically computed property keys — use `Map` or `Set`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_dynamic_delete() {
        let diags = run_on("delete obj[key];");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_dynamic_delete_expression() {
        let diags = run_on("delete obj[a + b];");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_static_string_delete() {
        assert!(run_on(r#"delete obj["foo"];"#).is_empty());
    }

    #[test]
    fn allows_static_number_delete() {
        assert!(run_on("delete obj[42];").is_empty());
    }

    #[test]
    fn allows_dot_property_delete() {
        assert!(run_on("delete obj.foo;").is_empty());
    }
}
