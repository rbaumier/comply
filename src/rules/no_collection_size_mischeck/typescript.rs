//! no-collection-size-mischeck — flag `.length >= 0` (always true)
//! and `.length < 0` (always false).
//!
//! Looks for `binary_expression` nodes where one side is a
//! `member_expression` accessing `.length` or `.size`, and the
//! other side is the literal `0`, combined with `>=` or `<`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let Ok(op) = op_node.utf8_text(source) else { return };

    if op != ">=" && op != "<" {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    // Check: left is `.length` or `.size` member_expression, right is `0`
    let is_size_prop = is_length_or_size(&left, source);
    let is_zero_right = right.kind() == "number" && right.utf8_text(source).ok() == Some("0");

    if !is_size_prop || !is_zero_right {
        return;
    }

    let desc = if op == ">=" { "always true" } else { "always false" };
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-collection-size-mischeck".into(),
        message: format!(
            "This collection size check is {} — `.length` and `.size` are never negative.",
            desc
        ),
        severity: Severity::Error,
        span: None,
    });
}

fn is_length_or_size(node: &tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = node.child_by_field_name("property") else { return false };
    let Ok(name) = prop.utf8_text(source) else { return false };
    name == "length" || name == "size"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_length_gte_zero() {
        assert_eq!(run_on("if (arr.length >= 0) {}").len(), 1);
    }

    #[test]
    fn flags_length_lt_zero() {
        assert_eq!(run_on("if (arr.length < 0) {}").len(), 1);
    }

    #[test]
    fn flags_size_gte_zero() {
        assert_eq!(run_on("if (set.size >= 0) {}").len(), 1);
    }

    #[test]
    fn allows_length_gt_zero() {
        assert!(run_on("if (arr.length > 0) {}").is_empty());
    }

    #[test]
    fn allows_length_eq_zero() {
        assert!(run_on("if (arr.length === 0) {}").is_empty());
    }
}
