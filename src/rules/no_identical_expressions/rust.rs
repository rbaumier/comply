//! no-identical-expressions Rust backend.
//!
//! Flag `expr OP expr` where both sides are identical.
//!
//! `==` / `!=` are excluded: `x != x` (and `x == x`) is the canonical IEEE 754
//! NaN-detection idiom — for floats, `x != x` is true iff `x` is NaN, which is
//! the only value not equal to itself. Rust AstCheck has no type inference, so
//! the operand cannot be proven to be a float; the `==`/`!=` self-comparison
//! form is overwhelmingly this idiom, so it is exempted in general (a deliberate
//! precision-over-recall tradeoff). Every other identical-operand expression
//! (`a && a`, `a - a`, `a / a`, …) is still flagged.
//!
//! Identical source text proves identical values only for reproducible
//! operands, so both sides must satisfy
//! [`rust_helpers::expression_is_reproducible`](crate::rules::rust_helpers::expression_is_reproducible).
//! `chars.next().is_some_and(f) && chars.next().is_some_and(f)` reads two
//! different characters: the calls advance the iterator between the two reads.
//!
//! `-` and `/` are skipped when an operand is provably an `f32`/`f64`
//! ([`rust_helpers::expression_is_float`](crate::rules::rust_helpers::expression_is_float)).
//! Reducing `x - x` to `0` holds for finite `x` only and `x / x` to `1` for
//! finite non-zero `x` only; on a NaN or an infinity both yield NaN, which is
//! what musl's `(x - x) / (x - x)` computes on purpose. An operand the AST
//! cannot type stays flagged: the proof runs in one direction only.
//!
//! `#[allow(clippy::eq_op)]` / `#[expect(clippy::eq_op)]` on any enclosing scope
//! suppresses the diagnostic, whatever the operator. `eq_op` is the upstream
//! lint this rule mirrors, so the author's in-source suppression of it answers
//! this rule too.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    expression_is_float, expression_is_reproducible, is_suppressed_by_clippy_allow,
};

const FLAGGED_OPS: &[&str] = &["&&", "||", "-", "/"];

/// The upstream clippy lint this rule mirrors on Rust: it reports the same
/// identical-operand expressions, so an author who has already answered it
/// in-source has answered this rule.
const MIRRORED_CLIPPY_LINTS: &[&str] = &["eq_op"];

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let Ok(op) = op_node.utf8_text(source) else { return };

    if !FLAGGED_OPS.contains(&op) {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let Ok(left_text) = left.utf8_text(source) else { return };
    let Ok(right_text) = right.utf8_text(source) else { return };

    if left_text != right_text
        || !expression_is_reproducible(left)
        || !expression_is_reproducible(right)
    {
        return;
    }

    // Float arithmetic does not reduce: `x - x` and `x / x` are NaN whenever `x`
    // is NaN or infinite, and `0.0 / 0.0` is the signalling-NaN idiom.
    if (op == "-" || op == "/")
        && (expression_is_float(left, source) || expression_is_float(right, source))
    {
        return;
    }

    if is_suppressed_by_clippy_allow(node, MIRRORED_CLIPPY_LINTS, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-identical-expressions".into(),
        message: format!(
            "Identical expression `{}` on both sides of `{}`.",
            left_text, op
        ),
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

    // `x != x` / `x == x` is the IEEE 754 NaN-detection idiom (`is_nan`), not a
    // duplicate-operand bug. See issue #5788 (rust-num/num-traits float.rs).
    #[test]
    fn allows_self_inequality_nan_idiom() {
        assert!(run_on("fn is_nan(self) -> bool { self != self }").is_empty());
    }

    #[test]
    fn allows_self_equality_nan_idiom() {
        assert!(run_on("fn not_nan(x: f64) -> bool { x == x }").is_empty());
    }

    #[test]
    fn allows_identical_eq() {
        assert!(run_on("fn f() { if a == a {} }").is_empty());
    }

    #[test]
    fn flags_identical_sub() {
        let d = run_on("fn f() { let z = total - total; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("-"));
    }

    #[test]
    fn flags_identical_div() {
        let d = run_on("fn f() { let r = total / total; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("/"));
    }

    #[test]
    fn flags_identical_and() {
        let d = run_on("fn f() { let ok = valid && valid; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("&&"));
    }

    #[test]
    fn flags_identical_or() {
        let d = run_on("fn f() { let ok = valid || valid; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("||"));
    }

    #[test]
    fn allows_different_sides() {
        assert!(run_on("fn f() { if a == b {} }").is_empty());
    }

    // Issue #6853 (astral-sh/uv exclude_newer.rs): each `chars.next()` consumes
    // the next character, so the two identical texts read different values.
    #[test]
    fn allows_repeated_iterator_next_calls() {
        let src = "fn f(after_sign: &str) -> bool { let mut chars = after_sign.chars(); \
                   chars.next().is_some_and(|c| c.is_ascii_digit()) \
                   && chars.next().is_some_and(|c| c.is_ascii_digit()) }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Issue #6853 (astral-sh/uv metadata.rs): same shape on a `Split` iterator.
    #[test]
    fn allows_repeated_split_next_calls() {
        let src = "fn f(s: &str) -> bool { let mut parts = s.split(\" :: \"); \
                   parts.next().is_some_and(|p| !p.is_empty()) \
                   && parts.next().is_some_and(|p| !p.is_empty()) }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A macro body stays unexpanded tokens, so the walker cannot see the
    // `next()` inside it — the operand is not provably reproducible.
    #[test]
    fn allows_identical_macro_invocations() {
        let src = "fn f(it: &mut I) -> bool { matches!(it.next(), Some(_)) \
                   && matches!(it.next(), Some(_)) }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A call-free operand still fires, so the silence above comes from the call
    // and not from the operator.
    #[test]
    fn flags_identical_call_free_operands_of_the_same_shape() {
        let d = run_on("fn f(a: bool, b: bool) -> bool { a && a }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_identical_field_access() {
        let d = run_on("fn f(&self) -> u32 { self.count - self.count }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_identical_index_expressions() {
        let d = run_on("fn f(xs: &[u32]) -> u32 { xs[0] / xs[0] }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_identical_cast_operands() {
        let d = run_on("fn f(n: u8) -> u32 { n as u32 - n as u32 }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_identical_scoped_constants() {
        let d = run_on("fn f() -> u32 { Limits::MAX - Limits::MAX }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_identical_dereference_operands() {
        let d = run_on("fn f(flag: &bool) -> bool { *flag && *flag }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_identical_borrowed_index_operands() {
        let d = run_on("fn f(map: &Map, key: Key) -> u32 { map[&key] - map[&key] }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_identical_slice_range_operands() {
        let d = run_on("fn f(buf: &[u8], n: usize) -> bool { buf[..n] && buf[..n] }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // diesel test code intentionally uses `value - value` to verify SQL null
    // propagation (issue #1500). `skip_in_test_dir` must suppress the rule there.
    #[test]
    fn skips_identical_operands_in_test_dir() {
        let src = "fn f() { let data = nullable_table.select(value - value).load(connection); }";
        let d = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "diesel_tests/tests/expressions/ops.rs",
        );
        assert!(d.is_empty(), "rule must be suppressed in a test directory");
    }

    #[test]
    fn flags_identical_operands_in_non_test_dir() {
        let src = "fn f() { let data = nullable_table.select(value - value).load(connection); }";
        let d = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/ops.rs");
        assert_eq!(d.len(), 1, "rule must still fire outside test directories");
        assert!(d[0].message.contains("-"));
    }

    // Issue #8250 (mooman219/fontdue src/platform/float/sqrt.rs): musl's
    // signalling-NaN idiom. On floats `x - x` is `0` only for finite `x` and
    // `x / x` is `1` only for finite non-zero `x`, so nothing reduces.
    #[test]
    fn allows_self_arithmetic_on_provably_float_operands() {
        for src in [
            "pub fn snan(x: f32) -> f32 { (x - x) / (x - x) }",
            "pub fn ratio(x: f64) -> f64 { x / x }",
            "pub fn ratio2(xy: f64) -> f64 { xy / xy }",
            "pub fn delta(x: f32) -> f32 { x - x }",
            "pub fn f(a: f64, b: f64) -> f64 { (a - b) / (a - b) }",
            "pub fn scaled(n: u32) -> f64 { n as f64 - n as f64 }",
            "pub fn literal() -> f64 { 1.5 - 1.5 }",
        ] {
            assert!(run_on(src).is_empty(), "{src}: {:?}", run_on(src));
        }
    }

    // Issue #8250: the verdict must not depend on how many characters the
    // operand's name has — `n - n` and `count - count` are the same expression.
    #[test]
    fn flags_self_arithmetic_on_integers_whatever_the_name_length() {
        for src in [
            "pub fn zero(count: i64) -> i64 { count - count }",
            "pub fn one(count: i64) -> i64 { count / count }",
            "pub fn zero1(n: i64) -> i64 { n - n }",
            "pub fn one1(n: i64) -> i64 { n / n }",
        ] {
            assert_eq!(run_on(src).len(), 1, "{src}: {:?}", run_on(src));
        }
    }

    // Issue #8250: `clippy::eq_op` is the upstream lint this rule mirrors, so the
    // author's in-source suppression of it answers this rule too. Integer
    // operands throughout, so the silence comes from the attribute and not from
    // the float exemption.
    #[test]
    fn honors_clippy_eq_op_suppression_at_every_scope() {
        for src in [
            // Statement scope.
            "fn f(count: i64) -> i64 { #[allow(clippy::eq_op)] return count - count; }",
            "fn f(count: i64) -> i64 { #[allow(clippy::eq_op)] let z = count - count; z }",
            // Function scope.
            "#[allow(clippy::eq_op)] fn f(count: i64) -> i64 { count - count }",
            // `impl` scope.
            "#[allow(clippy::eq_op)] impl S { fn f(&self, count: i64) -> i64 { count - count } }",
            // Crate root.
            "#![allow(clippy::eq_op)]\nfn f(count: i64) -> i64 { count - count }",
            // `#[expect]` is the same opt-out.
            "#[expect(clippy::eq_op)] fn f(count: i64) -> i64 { count - count }",
        ] {
            assert!(run_on(src).is_empty(), "{src}: {:?}", run_on(src));
        }
    }

    #[test]
    fn ignores_an_allow_of_an_unrelated_clippy_lint() {
        let src = "#[allow(clippy::needless_return)] fn f(count: i64) -> i64 { count - count }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }
}
