//! operation-returning-nan Rust backend.
//!
//! Flags arithmetic with `f64::NAN` / `f32::NAN` or division by zero literal.
//! Rust doesn't have `undefined` or implicit string coercion, but NaN
//! propagation is still a footgun.
//!
//! Test code is exempt: deliberately producing NaN / dividing by zero is a
//! valid way to exercise NaN-robustness.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir};

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    // Skip test code — deliberately constructing NaN / dividing by zero is a
    // valid technique for exercising NaN-robustness (e.g. `#[should_panic]`).
    if is_in_test_context(node, source) || is_under_tests_dir(ctx.path) {
        return;
    }

    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let left_text = left.utf8_text(source).unwrap_or("");
    let right_text = right.utf8_text(source).unwrap_or("");

    // NaN arithmetic — match only the exact f64::NAN / f32::NAN constants.
    let has_nan = matches!(left_text, "f64::NAN" | "f32::NAN")
        || matches!(right_text, "f64::NAN" | "f32::NAN");

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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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

    #[test]
    fn allows_constant_with_nan_in_name() {
        let source = r#"
            const MAX_SPAN_NANOSECONDS: i128 = 86400000000000i128;
            fn test() {
                let nanos = i128::from(MAX_SPAN_NANOSECONDS) + 2;
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_nanoseconds_variable() {
        let source = r#"
            fn test() {
                let ns = 1_000_000_000i64;
                let total_ns = ns * 7;
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_deliberate_nan_in_test_fn() {
        // amethyst/amethyst transform_system.rs — `#[should_panic]` tests feed
        // NaN / inf to verify the transform system rejects them.
        let source = r#"
            #[test]
            #[should_panic]
            fn nan_transform() {
                local.set_translation_xyz(0.0 / 0.0, 0.0 / 0.0, 0.0 / 0.0);
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_div_by_zero_in_cfg_test_module() {
        let source = r#"
            #[cfg(test)]
            mod tests {
                fn build() {
                    let x = 1.0 / 0.0;
                    let y = f64::NAN + 1.0;
                }
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_nan_in_production_fn() {
        // A NaN-producing operation in non-test code remains flagged.
        let source = r#"
            fn compute() {
                let x = f64::NAN + 1.0;
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }
}
