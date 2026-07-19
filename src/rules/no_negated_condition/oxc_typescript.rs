//! no-negated-condition OxcCheck backend — flag negated conditions in
//! if/else (only when the negated branch is at least as large as the else)
//! and ternaries. Two idiomatic shapes are exempt: a ternary whose arm renders
//! JSX (`cond ? <X/> : null`), and any inequality against a presence sentinel
//! (`x !== null` / `x !== undefined` / `x !== 0` / `typeof x !== 'undefined'`),
//! where the negation is the natural check and inverting the arms reads worse.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement, AstType::ConditionalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            oxc_ast::AstKind::IfStatement(stmt) => {
                // Must have an else branch.
                let Some(ref alt) = stmt.alternate else {
                    return;
                };
                // Skip `else if` chains.
                if matches!(alt, Statement::IfStatement(_)) {
                    return;
                }
                // `x !== null` / `x !== undefined` / `x !== 0` read as idiomatic
                // "is present / non-zero" checks, not swappable logic branches —
                // see `is_sentinel_inequality`.
                if is_sentinel_inequality(&stmt.test) {
                    return;
                }
                if is_negated_expr(&stmt.test)
                    && branch_size(&stmt.consequent) >= branch_size(alt)
                {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, stmt.test.span().start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Unexpected negated condition — swap the if/else branches \
                                  and remove the negation."
                            .into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            oxc_ast::AstKind::ConditionalExpression(expr) => {
                // JSX conditional rendering (`cond ? <X/> : null`,
                // `cond ? null : <X/>`, `cond ? <A/> : <B/>`) reads as a
                // "render when" guard, not a logic branch: the `!== null`
                // negation is the natural presence check, so inverting the arms
                // would read worse. Skip when either arm renders JSX.
                if arm_renders_jsx(&expr.consequent) || arm_renders_jsx(&expr.alternate) {
                    return;
                }
                // `x !== null` / `x !== undefined` / `x !== 0` — idiomatic
                // presence / non-zero checks; inverting the arms reads worse.
                if is_sentinel_inequality(&expr.test) {
                    return;
                }
                if is_negated_expr(&expr.test) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, expr.test.span().start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Unexpected negated condition — swap the ternary arms \
                                  and remove the negation."
                            .into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            _ => {}
        }
        let _ = semantic;
    }
}

/// Number of statements in an `if`/`else` branch — a block's length, or 1 for
/// a single unbraced statement. The rule fires only when the negated (`if`)
/// branch carries at least as much code as the `else`: that is the case where
/// the main logic sits under the negation and swapping clarifies intent. A
/// small negated branch with a larger `else` reads as a guard, not a win.
fn branch_size(stmt: &Statement) -> usize {
    match stmt {
        Statement::BlockStatement(block) => block.body.len(),
        _ => 1,
    }
}

/// True when a ternary arm renders JSX — a `<Foo/>` element or a `<>…</>`
/// fragment (ignoring wrapping parentheses). The `!== null` negation guarding
/// such an arm is the natural presence check, not a swappable logic branch.
fn arm_renders_jsx(expr: &Expression) -> bool {
    matches!(
        expr.without_parentheses(),
        Expression::JSXElement(_) | Expression::JSXFragment(_)
    )
}

/// True when `expr` is an inequality (`!=` / `!==`) against a "presence"
/// sentinel — `null`, `undefined`, `0`, or the `typeof x !== 'undefined'`
/// shape. `x !== null` / `idx !== 0` read as idiomatic "is present / non-zero"
/// checks, so the negation is natural and inverting the if/else or ternary arms
/// reads worse. Note: `-1` (the `indexOf` "not found" sentinel) is deliberately
/// NOT covered — that comparison belongs to `String#includes` /
/// `Array#includes`, enforced by `prefer-includes`.
fn is_sentinel_inequality(expr: &Expression) -> bool {
    let Expression::BinaryExpression(binary) = expr.without_parentheses() else {
        return false;
    };
    if !matches!(
        binary.operator,
        BinaryOperator::Inequality | BinaryOperator::StrictInequality
    ) {
        return false;
    }
    is_presence_sentinel(&binary.left)
        || is_presence_sentinel(&binary.right)
        || is_typeof_undefined(&binary.left, &binary.right)
}

/// `null`, the `undefined` identifier, or the numeric literal `0`.
fn is_presence_sentinel(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::NullLiteral(_) => true,
        Expression::Identifier(id) => id.name.as_str() == "undefined",
        Expression::NumericLiteral(num) => num.value == 0.0,
        _ => false,
    }
}

/// True for the AST shape `typeof X !== 'undefined'` (either operand order) —
/// one side is a `typeof` unary expression, the other the string literal
/// `'undefined'`. `'undefined'` is the language-defined `typeof` result for an
/// undeclared/undefined binding, making this the only safe runtime-presence
/// check when `X` may not be declared; it is the structural parallel of the
/// `!== undefined` identifier sentinel.
fn is_typeof_undefined(left: &Expression, right: &Expression) -> bool {
    typeof_then_undefined_string(left, right) || typeof_then_undefined_string(right, left)
}

/// True when `typeof_side` is a `typeof` unary expression and `string_side` is
/// the string literal `'undefined'`.
fn typeof_then_undefined_string(typeof_side: &Expression, string_side: &Expression) -> bool {
    let Expression::UnaryExpression(unary) = typeof_side.without_parentheses() else {
        return false;
    };
    if unary.operator != UnaryOperator::Typeof {
        return false;
    }
    matches!(
        string_side.without_parentheses(),
        Expression::StringLiteral(s) if s.value.as_str() == "undefined"
    )
}

/// A condition is "negated" if it is:
/// - a `!expr` unary expression whose leading `!` chain has ODD length, OR
/// - a `!=` / `!==` binary expression.
///
/// An even-length `!` chain (`!!x`) is idiomatic boolean coercion: the
/// negations cancel to a positive condition, so it is not treated as negated.
fn is_negated_expr(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::UnaryExpression(u) if u.operator == UnaryOperator::LogicalNot => {
            leading_not_count(expr) % 2 == 1
        }
        Expression::BinaryExpression(b) => {
            matches!(
                b.operator,
                BinaryOperator::Inequality | BinaryOperator::StrictInequality
            )
        }
        _ => false,
    }
}

/// Length of the leading consecutive `!` (`LogicalNot`) chain, ignoring any
/// parentheses between the negations: `!x` → 1, `!!x` → 2, `!!!x` → 3.
fn leading_not_count(expr: &Expression) -> usize {
    match expr.without_parentheses() {
        Expression::UnaryExpression(u) if u.operator == UnaryOperator::LogicalNot => {
            1 + leading_not_count(&u.argument)
        }
        _ => 0,
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_tsx(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_negated_if_else() {
        let d = run_on("if (!x) { a(); } else { b(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("swap the if/else"));
    }

    #[test]
    fn flags_not_equal_if_else() {
        let d = run_on("if (a !== b) { x(); } else { y(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_loose_not_equal_if_else() {
        let d = run_on("if (a != b) { x(); } else { y(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_if_without_else() {
        assert!(run_on("if (!x) { a(); }").is_empty());
    }

    #[test]
    fn allows_negated_if_smaller_than_else() {
        // Small negated guard, larger else — swapping would bury the main
        // branch under the negation rather than clarify it.
        assert!(run_on("if (!x) { return; } else { a(); b(); c(); }").is_empty());
    }

    #[test]
    fn flags_negated_if_larger_than_else() {
        let d = run_on("if (!x) { a(); b(); c(); } else { d(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_else_if() {
        assert!(run_on("if (!x) { a(); } else if (y) { b(); }").is_empty());
    }

    #[test]
    fn allows_positive_condition() {
        assert!(run_on("if (x) { a(); } else { b(); }").is_empty());
    }

    #[test]
    fn flags_negated_ternary() {
        let d = run_on("const r = !x ? a : b;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("swap the ternary"));
    }

    #[test]
    fn flags_not_equal_ternary() {
        let d = run_on("const r = a !== b ? x : y;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_positive_ternary() {
        assert!(run_on("const r = x ? a : b;").is_empty());
    }

    // Double negation (`!!x`) is idiomatic boolean coercion — an even-length
    // `!` chain is a positive condition, so it is not flagged. Only an
    // odd-length chain (`!x`, `!!!x`) reads as a real negation.
    #[test]
    fn allows_double_negation_ternary() {
        assert!(run_on("const r = !!options.replace ? a : b;").is_empty());
    }

    #[test]
    fn allows_double_negation_if_else() {
        assert!(run_on("if (!!x) { f(); } else { g(); }").is_empty());
    }

    #[test]
    fn allows_parenthesized_double_negation_ternary() {
        // `!(!x)` — parentheses between the negations don't change the parity:
        // an even count of two, still a positive condition.
        assert!(run_on("const r = !(!x) ? a : b;").is_empty());
    }

    #[test]
    fn flags_triple_negation_ternary() {
        // `!!!x` is an odd-length chain — still a negated condition.
        let d = run_on("const r = !!!x ? a : b;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_single_negation_of_logical_and() {
        // `!(a && b)` is a single `!` whose operand is not another `!` — an
        // odd-length chain of one, still a flagged negated condition.
        let d = run_on("const r = !(a && b) ? a : b;");
        assert_eq!(d.len(), 1);
    }

    // JSX conditional rendering — a negated test guarding a JSX arm reads as a
    // "render when" presence check, not a swappable logic branch. See the
    // ConditionalExpression arm in `run`.
    #[test]
    fn allows_negated_ternary_with_jsx_consequent_and_null() {
        // `cond ? <X/> : null` — the canonical "render when present" guard.
        assert!(run_tsx("const r = x !== null ? <Foo /> : null;").is_empty());
        assert!(run_tsx("const r = !x ? <Foo /> : null;").is_empty());
    }

    #[test]
    fn allows_negated_ternary_with_jsx_alternate() {
        // `cond ? null : <X/>` — JSX on the else arm.
        assert!(run_tsx("const r = !x ? null : <Foo />;").is_empty());
    }

    #[test]
    fn allows_negated_ternary_with_both_jsx_arms() {
        assert!(run_tsx("const r = !x ? <A /> : <B />;").is_empty());
    }

    #[test]
    fn allows_negated_ternary_with_parenthesized_jsx() {
        assert!(run_tsx("const r = x !== null ? (<Foo />) : null;").is_empty());
    }

    #[test]
    fn flags_negated_ternary_with_non_jsx_arms_in_tsx() {
        // A value-choosing ternary is still flagged even inside a `.tsx` file:
        // the exemption is about rendering JSX, not about the file extension.
        let d = run_tsx("const r = !x ? a : b;");
        assert_eq!(d.len(), 1);
    }

    // Presence-sentinel inequalities (`!== null` / `!== undefined` / `!== 0`)
    // read as idiomatic "is present / non-zero" checks — exempt in both
    // if/else and ternary. See `is_sentinel_inequality`.
    #[test]
    fn allows_null_sentinel_inequality() {
        assert!(run_on("if (x !== null) { a(); } else { b(); }").is_empty());
        assert!(run_on("const r = x !== null ? a : b;").is_empty());
        assert!(run_on("const r = null !== x ? a : b;").is_empty());
    }

    #[test]
    fn allows_undefined_sentinel_inequality() {
        assert!(run_on("if (x !== undefined) { a(); } else { b(); }").is_empty());
        assert!(run_on("const r = x !== undefined ? a : b;").is_empty());
    }

    #[test]
    fn allows_typeof_undefined_inequality() {
        // `typeof x !== 'undefined'` is the canonical runtime-presence check —
        // the only safe form when `x` may be undeclared — exempt in both
        // if/else and ternary, in either operand order.
        assert!(run_on("if (typeof atob !== 'undefined') { a(); } else { b(); }").is_empty());
        assert!(
            run_on("const v = typeof data[key] !== 'undefined' ? data[key] : fallback;")
                .is_empty()
        );
        assert!(run_on("const v = 'undefined' !== typeof atob ? a : b;").is_empty());
        // Loose `!=` too — `typeof` always yields a string, so it is equivalent.
        assert!(run_on("if (typeof atob != 'undefined') { a(); } else { b(); }").is_empty());
    }

    #[test]
    fn flags_non_undefined_typeof_string_inequality() {
        // `typeof x !== 'foo'` (or any non-'undefined' string) is a real
        // negated comparison, not the presence sentinel — still flagged.
        let d = run_on("if (typeof x !== 'function') { a(); } else { b(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_plain_string_inequality_not_via_typeof() {
        // A bare string compare (no `typeof`) is not the presence sentinel —
        // still a flagged negated condition.
        let d = run_on("if (x !== 'foo') { a(); y(); } else { b(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_zero_sentinel_inequality() {
        // The comparator tiebreak idiom `cmp !== 0 ? cmp : fallback`.
        assert!(run_on("const r = cmp !== 0 ? cmp : fallback;").is_empty());
        assert!(run_on("if (cmp != 0) { a(); } else { b(); }").is_empty());
    }

    #[test]
    fn flags_minus_one_inequality_not_a_sentinel() {
        // `idx !== -1` is the `indexOf` "not found" sentinel — that belongs to
        // `prefer-includes`, not here, so the negation is still flagged.
        let d = run_on("const r = idx !== -1 ? a : b;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_non_sentinel_inequality() {
        // A non-sentinel comparison (`a !== b`) is still a flagged negated
        // condition — the exemption is narrow to presence sentinels.
        let d = run_on("if (a !== b) { x(); y(); } else { z(); }");
        assert_eq!(d.len(), 1);
    }
}
