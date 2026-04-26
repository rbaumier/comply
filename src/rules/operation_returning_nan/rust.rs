//! operation-returning-nan Rust backend.
//!
//! Flags arithmetic with `f64::NAN` / `f32::NAN` or division by zero literal.
//! Rust doesn't have `undefined` or implicit string coercion, but NaN
//! propagation is still a footgun.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let left_text = left.utf8_text(source).unwrap_or("");
    let right_text = right.utf8_text(source).unwrap_or("");

    // NaN arithmetic.
    let has_nan = left_text.contains("NAN") || right_text.contains("NAN")
        || left_text.contains("NaN") || right_text.contains("NaN");

    if has_nan && matches!(op, "+" | "-" | "*" | "/") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "operation-returning-nan".into(),
            message: "Arithmetic with NaN will propagate NaN.".into(),
            severity: Severity::Error,
            span: None,
        });
        return;
    }

    // Division by zero literal.
    if op == "/" && (right_text == "0" || right_text == "0.0") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "operation-returning-nan".into(),
            message: "Division by zero literal.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_nan_arithmetic() {
        assert_eq!(run_on("fn f() { let x = f64::NAN + 1.0; }").len(), 1);
    }

    #[test]
    fn flags_div_by_zero() {
        assert_eq!(run_on("fn f() { let x = 10.0 / 0.0; }").len(), 1);
    }

    #[test]
    fn allows_normal_arithmetic() {
        assert!(run_on("fn f() { let x = 1.0 + 2.0; }").is_empty());
    }
}
