//! no-negated-condition OxcCheck backend — flag negated conditions in
//! if/else (only when the negated branch is at least as large as the else)
//! and ternaries. Two idiomatic shapes are exempt: a ternary whose arm renders
//! JSX (`cond ? <X/> : null`), and any inequality against a presence sentinel
//! (`x !== null` / `x !== undefined` / `x !== 0`), where the negation is the
//! natural check and inverting the arms reads worse.

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
                        severity: Severity::Warning,
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
                        severity: Severity::Warning,
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
/// sentinel — `null`, `undefined`, or `0`. `x !== null` / `idx !== 0` read as
/// idiomatic "is present / non-zero" checks, so the negation is natural and
/// inverting the if/else or ternary arms reads worse. Note: `-1` (the `indexOf`
/// "not found" sentinel) is deliberately NOT covered — that comparison belongs
/// to `String#includes` / `Array#includes`, enforced by `prefer-includes`.
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
    is_presence_sentinel(&binary.left) || is_presence_sentinel(&binary.right)
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

/// A condition is "negated" if it is:
/// - a `!expr` unary expression, OR
/// - a `!=` / `!==` binary expression.
fn is_negated_expr(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::UnaryExpression(u) => u.operator == UnaryOperator::LogicalNot,
        Expression::BinaryExpression(b) => {
            matches!(
                b.operator,
                BinaryOperator::Inequality | BinaryOperator::StrictInequality
            )
        }
        _ => false,
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
