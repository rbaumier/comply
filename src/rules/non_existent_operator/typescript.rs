//! non-existent-operator — detect typo operators `=+`, `=-`, `=!`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // We look for assignment_expression nodes where the right side starts
    // with a unary +, -, or ! — this is the AST shape produced by `x =+ 1`,
    // `x =- 1`, `x =! y`.  The user likely meant `+=`, `-=`, `!=`.
    //
    // For `=+` and `=-`: tree-sitter parses `x =+ 1` as an assignment
    // where the RHS is a unary_expression (`+1`).
    // For `=!`: tree-sitter parses `x =! y` as an assignment where the
    // RHS is a unary_expression (`!y`).
    if node.kind() != "assignment_expression" {
        return;
    }

    // Must be a plain `=` assignment (not `+=`, `-=`, etc.)
    let Some(op_node) = node.child(1) else { return };
    let op_text = op_node.utf8_text(source).unwrap_or("");
    if op_text != "=" {
        return;
    }

    let Some(rhs) = node.child_by_field_name("right") else { return };
    if rhs.kind() != "unary_expression" {
        return;
    }

    // The unary operator is the first child of the unary_expression.
    let Some(unary_op) = rhs.child(0) else { return };
    let unary_text = unary_op.utf8_text(source).unwrap_or("");
    if unary_text != "+" && unary_text != "-" && unary_text != "!" {
        return;
    }

    // Check adjacency: the `=` and the unary op must be adjacent (no space)
    // to distinguish `x =+1` (typo) from `x = +1` (intentional).
    let eq_end = op_node.end_byte();
    let unary_start = unary_op.start_byte();
    if unary_start != eq_end {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "non-existent-operator".into(),
        message: "Typo operator — did you mean `+=`, `-=`, or `!=`?".into(),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts;

    #[test]
    fn flags_equals_plus() {
        let d = run_ts("x =+ 1;", &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "non-existent-operator");
    }

    #[test]
    fn flags_equals_minus() {
        let d = run_ts("x =- 1;", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_equals_bang() {
        let d = run_ts("x =! true;", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_plus_equals() {
        assert!(run_ts("x += 1;", &Check).is_empty());
    }

    #[test]
    fn allows_minus_equals() {
        assert!(run_ts("x -= 1;", &Check).is_empty());
    }

    #[test]
    fn allows_not_equals() {
        assert!(run_ts("if (x !== y) {}", &Check).is_empty());
        assert!(run_ts("if (x != y) {}", &Check).is_empty());
    }

    #[test]
    fn allows_unary_with_space() {
        assert!(run_ts("x = +1;", &Check).is_empty());
        assert!(run_ts("x = -1;", &Check).is_empty());
        assert!(run_ts("x = !true;", &Check).is_empty());
    }
}
