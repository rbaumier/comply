//! index-of-compare-to-positive backend — `.indexOf(…) > 0` misses index 0.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    // Match binary expressions: `expr > 0` or `expr < 1`.
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");

    if op != ">" && op != "<" {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let right_text = right.utf8_text(source).unwrap_or("").trim();

    // `.indexOf(…) > 0` or `.indexOf(…) < 1`
    let is_bad = (op == ">" && right_text == "0") || (op == "<" && right_text == "1");
    if !is_bad {
        return;
    }

    // Check if left side is a `.indexOf(...)` call.
    if left.kind() != "call_expression" {
        return;
    }
    let Some(callee) = left.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let prop_name = prop.utf8_text(source).unwrap_or("");
    if prop_name != "indexOf" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "index-of-compare-to-positive".into(),
        message: "`.indexOf(…) > 0` misses index 0 — use `>= 0` or `!== -1`.".into(),
        severity: Severity::Error,
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
    fn flags_indexof_gt_zero() {
        assert_eq!(run_on("if (arr.indexOf(x) > 0) {}").len(), 1);
    }

    #[test]
    fn flags_indexof_lt_one() {
        assert_eq!(run_on("if (str.indexOf('a') < 1) {}").len(), 1);
    }

    #[test]
    fn allows_indexof_gte_zero() {
        assert!(run_on("if (arr.indexOf(x) >= 0) {}").is_empty());
    }

    #[test]
    fn allows_indexof_neq_minus_one() {
        assert!(run_on("if (arr.indexOf(x) !== -1) {}").is_empty());
    }
}
