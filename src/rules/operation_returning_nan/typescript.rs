//! operation-returning-nan — flag arithmetic that produces NaN.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the node is a string literal.
fn is_string(kind: &str) -> bool {
    kind == "string" || kind == "template_string"
}

/// Returns true if the node is `undefined`.
fn is_undefined(node: tree_sitter::Node, _source: &[u8]) -> bool {
    node.kind() == "undefined"
}

/// Returns true if the operator is arithmetic (not `+` which is also string concat).
fn is_arith_op(op: &str) -> bool {
    matches!(op, "-" | "*" | "/" | "%" | "**")
}

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    // undefined +/- /* // any  or  any +/- /* // undefined
    let has_undefined = is_undefined(left, source) || is_undefined(right, source);
    if has_undefined && (op == "+" || is_arith_op(op)) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "operation-returning-nan".into(),
            message: "Arithmetic with `undefined` will produce `NaN`.".into(),
            severity: Severity::Error,
            span: None,
        });
        return;
    }

    // "string" - /* // * any  or  any - /* // * "string"
    let has_string = is_string(left.kind()) || is_string(right.kind());
    if has_string && is_arith_op(op) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "operation-returning-nan".into(),
            message: "Arithmetic on a string literal will produce `NaN`.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts;

    #[test]
    fn flags_undefined_plus() {
        let d = run_ts("const x = undefined + 1;", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("undefined"));
    }

    #[test]
    fn flags_undefined_minus() {
        let d = run_ts("const x = undefined - 5;", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_string_multiply() {
        let d = run_ts("const x = \"hello\" * 2;", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("string"));
    }

    #[test]
    fn flags_string_minus() {
        let d = run_ts("const x = \"text\" - 1;", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_number_arithmetic() {
        assert!(run_ts("const x = 10 + 5;", &Check).is_empty());
    }

    #[test]
    fn allows_string_concat() {
        assert!(run_ts("const x = \"hello\" + \" world\";", &Check).is_empty());
    }
}
