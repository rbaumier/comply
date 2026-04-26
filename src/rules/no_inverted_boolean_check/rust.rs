//! no-inverted-boolean-check Rust backend — flag `!a == b` patterns.
//!
//! In Rust `!` binds tighter than `==`, so `!a == b` is `(!a) == b`,
//! not `!(a == b)`. This is almost always a mistake.
//!
//! Walks `binary_expression` nodes whose operator is `==` or `!=` and
//! flags the case where the left operand is a `unary_expression` with
//! the `!` operator.

use crate::diagnostic::{Diagnostic, Severity};

const EQUALITY_OPS: &[&str] = &["==", "!="];

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op) = node.child_by_field_name("operator") else { return };
    let Ok(op_text) = op.utf8_text(source) else { return };
    if !EQUALITY_OPS.contains(&op_text) {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    if left.kind() != "unary_expression" {
        return;
    }

    // The Rust grammar's `unary_expression` covers `!`, `-`, `*`. Confirm
    // the operator is the boolean-not by looking at the leading token text.
    let Ok(left_text) = left.utf8_text(source) else { return };
    if !left_text.starts_with('!') {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-inverted-boolean-check".into(),
        message: "`!a == b` negates `a` before comparing \u{2014} use `a != b` or `!(a == b)`.".into(),
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
    fn flags_not_a_equals_b() {
        assert_eq!(run_on("fn f(a: bool, b: bool) { if !a == b {} }").len(), 1);
    }

    #[test]
    fn flags_not_a_not_equals_b() {
        assert_eq!(run_on("fn f(a: bool, b: bool) { if !a != b {} }").len(), 1);
    }

    #[test]
    fn allows_normal_comparison() {
        assert!(run_on("fn f(a: i32, b: i32) { if a == b {} }").is_empty());
    }

    #[test]
    fn allows_negated_result() {
        assert!(run_on("fn f(a: i32, b: i32) { if !(a == b) {} }").is_empty());
    }
}
