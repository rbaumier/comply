//! prefer-array-some AST backend — flag `.filter(…).length > 0` etc.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "binary_expression" {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    // Get the operator.
    let mut cursor = node.walk();
    let op = node.children(&mut cursor)
        .find(|c| matches!(c.kind(), ">" | ">=" | "!==" | "!="))
        .map(|c| c.utf8_text(source).unwrap_or(""));
    let Some(op) = op else { return };

    let right_text = right.utf8_text(source).unwrap_or("").trim();

    // Check for: `.length > 0`, `.length !== 0`, `.length != 0`, `.length >= 1`
    let is_existence_check = match op {
        ">" => right_text == "0",
        "!==" | "!=" => right_text == "0",
        ">=" => right_text == "1",
        _ => false,
    };
    if !is_existence_check {
        return;
    }

    // Left side should be `<expr>.length` where `<expr>` is `.filter(…)`.
    if left.kind() != "member_expression" {
        return;
    }
    let Some(length_prop) = left.child_by_field_name("property") else { return };
    if length_prop.utf8_text(source).unwrap_or("") != "length" {
        return;
    }

    let Some(filter_call) = left.child_by_field_name("object") else { return };
    if filter_call.kind() != "call_expression" {
        return;
    }

    let Some(filter_callee) = filter_call.child_by_field_name("function") else { return };
    if filter_callee.kind() != "member_expression" {
        return;
    }
    let Some(filter_prop) = filter_callee.child_by_field_name("property") else { return };
    if filter_prop.utf8_text(source).unwrap_or("") != "filter" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-array-some".into(),
        message: "Prefer `.some(…)` over `.filter(…).length` check — `.some()` short-circuits.".into(),
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
    fn flags_filter_length_gt_zero() {
        assert_eq!(run_on("if (arr.filter(fn).length > 0) {}").len(), 1);
    }

    #[test]
    fn flags_filter_length_not_equal_zero() {
        assert_eq!(run_on("if (arr.filter(fn).length !== 0) {}").len(), 1);
    }

    #[test]
    fn flags_filter_length_gte_one() {
        assert_eq!(run_on("if (arr.filter(fn).length >= 1) {}").len(), 1);
    }

    #[test]
    fn allows_some() {
        assert!(run_on("if (arr.some(fn)) {}").is_empty());
    }

    #[test]
    fn allows_filter_length_alone() {
        assert!(run_on("const n = arr.filter(fn).length;").is_empty());
    }
}
