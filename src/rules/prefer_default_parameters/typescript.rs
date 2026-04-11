//! prefer-default-parameters backend — flag `x = x || 'default'` / `x = x ?? 'default'`.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node is a literal value (string, number, boolean, null, undefined).
fn is_literal(node: tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "string" | "template_string" | "number" | "true" | "false" | "null" | "undefined"
    )
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Look for assignment expressions: `x = x || 'default'` or `x = x ?? 'default'`
    if node.kind() != "assignment_expression" {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    // Left must be a simple identifier
    if left.kind() != "identifier" {
        return;
    }
    let lhs_name = left.utf8_text(source).unwrap_or("");

    // Right must be a binary_expression with `||` or `??`
    if right.kind() != "binary_expression" {
        return;
    }

    let Some(op_node) = right.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");
    if op != "||" && op != "??" {
        return;
    }

    let Some(rl) = right.child_by_field_name("left") else { return };
    let Some(rr) = right.child_by_field_name("right") else { return };

    // Left side of || / ?? must be the same identifier
    if rl.kind() != "identifier" || rl.utf8_text(source).unwrap_or("") != lhs_name {
        return;
    }

    // Right side must be a literal
    if !is_literal(rr) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-default-parameters".into(),
        message: "Prefer default parameters over reassignment.".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_logical_or_reassignment() {
        let d = run_on("function f(x) {\n  x = x || 'default';\n}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-default-parameters");
    }

    #[test]
    fn flags_nullish_coalescing_reassignment() {
        let d = run_on("function f(x) {\n  x = x ?? 42;\n}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_default_parameter() {
        assert!(run_on("function f(x = 'default') {}").is_empty());
    }

    #[test]
    fn allows_different_identifiers() {
        assert!(run_on("function f(x) {\n  x = y || 'default';\n}").is_empty());
    }

    #[test]
    fn allows_non_literal_rhs() {
        assert!(run_on("function f(x) {\n  x = x || getValue();\n}").is_empty());
    }
}
