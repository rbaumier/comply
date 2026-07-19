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

    // In Rust `!` on an integer or byte is bitwise NOT, not logical NOT.
    // Comparing a `bool` to a numeric or char/byte literal is a type error, so
    // such a literal on the opposite side proves the `!` is bitwise complement
    // (`!digit != 0` means "digit is not all-ones", `!b == b'*'` compares a
    // byte's complement) — not the inverted-boolean footgun this targets. A
    // byte literal `b'*'` and a char literal `'*'` both parse as `char_literal`.
    let Some(right) = node.child_by_field_name("right") else { return };
    if matches!(right.kind(), "integer_literal" | "float_literal" | "char_literal") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-inverted-boolean-check".into(),
        message: "`!a == b` negates `a` before comparing \u{2014} use `a != b` or `!(a == b)`.".into(),
        severity: Severity::Error,
        span: None,
    });
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

    #[test]
    fn allows_bitwise_not_compared_to_integer_literal() {
        // `!digit` is bitwise complement here; comparing a bool to an integer
        // would be a type error, so `!` cannot be logical NOT.
        assert!(
            run_on("fn f() { let _ = [0u32].iter().position(|&digit| !digit != 0); }").is_empty()
        );
        assert!(run_on("fn f(x: u32) { if !x == 0 {} }").is_empty());
    }

    #[test]
    fn allows_bitwise_not_compared_to_float_literal() {
        assert!(run_on("fn f(x: u32) { if !x != 0.0 {} }").is_empty());
    }

    #[test]
    fn allows_bitwise_not_compared_to_byte_literal() {
        // `b` is a `u8`, so `!b` is bitwise complement; a byte literal on the
        // opposite side of `==` cannot be compared to a bool, proving `!` is not
        // logical NOT here.
        assert!(run_on("fn f(b: u8) -> bool { !b == b'*' }").is_empty());
    }

    #[test]
    fn allows_bitwise_not_compared_to_char_literal() {
        assert!(run_on("fn g(c: u8) -> bool { !c == '*' }").is_empty());
    }

    #[test]
    fn flags_not_flag_equals_bool_literal() {
        assert_eq!(run_on("fn f(flag: bool) { if !flag == false {} }").len(), 1);
    }

    #[test]
    fn flags_not_ready_not_equals_bool_literal() {
        assert_eq!(run_on("fn f(ready: bool) { if !ready != true {} }").len(), 1);
    }

    #[test]
    fn flags_not_flag_equals_non_literal_bool() {
        // The other operand is an identifier, not a numeric/char literal, so the
        // inverted-boolean footgun still applies.
        assert_eq!(
            run_on("fn h(flag: bool, other: bool) -> bool { !flag == other }").len(),
            1
        );
    }
}
