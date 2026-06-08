//! OXC backend for prefer-math-min-max.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BinaryOperator;
use oxc_span::GetSpan;
use std::sync::Arc;

fn is_numeric_literal(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::{Expression, UnaryOperator};
    match expr {
        Expression::NumericLiteral(_) => true,
        Expression::UnaryExpression(u) => {
            u.operator == UnaryOperator::UnaryNegation
                && matches!(u.argument, Expression::NumericLiteral(_))
        }
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ConditionalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ConditionalExpression(cond) = node.kind() else {
            return;
        };

        // The test must be a binary comparison.
        let oxc_ast::ast::Expression::BinaryExpression(test) = &cond.test else {
            return;
        };

        let op = test.operator;
        let is_gt = matches!(op, BinaryOperator::GreaterThan | BinaryOperator::GreaterEqualThan);
        let is_lt = matches!(op, BinaryOperator::LessThan | BinaryOperator::LessEqualThan);
        if !is_gt && !is_lt {
            return;
        }

        let left_text = &ctx.source[test.left.span().start as usize..test.left.span().end as usize];
        let right_text = &ctx.source[test.right.span().start as usize..test.right.span().end as usize];
        let cons_text = &ctx.source[cond.consequent.span().start as usize..cond.consequent.span().end as usize];
        let alt_text = &ctx.source[cond.alternate.span().start as usize..cond.alternate.span().end as usize];

        let left_text = left_text.trim();
        let right_text = right_text.trim();
        let cons_text = cons_text.trim();
        let alt_text = alt_text.trim();

        if left_text.is_empty() || right_text.is_empty() {
            return;
        }

        // Only fire when at least one operand is a numeric literal; skip string/branded-id
        // comparisons where Math.min/max would produce NaN instead of a lexicographic result.
        if !is_numeric_literal(&test.left) && !is_numeric_literal(&test.right) {
            return;
        }

        let method: Option<&str> = if (is_gt && left_text == alt_text && right_text == cons_text)
            || (is_lt && left_text == cons_text && right_text == alt_text)
        {
            Some("min")
        } else if (is_gt && left_text == cons_text && right_text == alt_text)
            || (is_lt && left_text == alt_text && right_text == cons_text)
        {
            Some("max")
        } else {
            None
        };

        if let Some(method) = method {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, cond.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Prefer `Math.{method}({left_text}, {right_text})` over this ternary."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    // Regression #756: both operands are identifiers (branded string ids) — must not fire.
    #[test]
    fn no_fp_on_identifier_only_ternary() {
        let src = r#"
            type GammeId = string & { readonly __brand: "GammeId" };
            function pick(a: GammeId, b: GammeId): GammeId {
                return a < b ? a : b;
            }
        "#;
        assert_eq!(run(src).len(), 0);
    }

    // Primary use case: one operand is a numeric literal — must fire.
    #[test]
    fn fires_on_numeric_literal_clamp() {
        let src = "const v = height < 50 ? height : 50;";
        assert_eq!(run(src).len(), 1);
    }

    // Both operands are numeric literals — must fire.
    #[test]
    fn fires_on_two_numeric_literals() {
        let src = "const v = 0 < 1 ? 0 : 1;";
        assert_eq!(run(src).len(), 1);
    }

    // Negative numeric literal — must fire.
    #[test]
    fn fires_on_negative_literal() {
        let src = "const v = x < -1 ? x : -1;";
        assert_eq!(run(src).len(), 1);
    }



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_gt_min_pattern() {
        // height > 50 ? 50 : height -> Math.min(height, 50)
        let d = run_on("const x = height > 50 ? 50 : height;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.min"));
    }


    #[test]
    fn flags_lt_min_pattern() {
        // height < 50 ? height : 50 -> Math.min(height, 50)
        let d = run_on("const x = height < 50 ? height : 50;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.min"));
    }


    #[test]
    fn flags_gte_min_pattern() {
        let d = run_on("const x = height >= 50 ? 50 : height;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.min"));
    }


    #[test]
    fn flags_gt_max_pattern() {
        // height > 50 ? height : 50 -> Math.max(height, 50)
        let d = run_on("const x = height > 50 ? height : 50;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.max"));
    }


    #[test]
    fn flags_lt_max_pattern() {
        // height < 50 ? 50 : height -> Math.max(height, 50)
        let d = run_on("const x = height < 50 ? 50 : height;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.max"));
    }


    #[test]
    fn allows_unrelated_ternary() {
        assert!(run_on("const x = a > b ? c : d;").is_empty());
    }


    #[test]
    fn allows_equality_ternary() {
        assert!(run_on("const x = a === b ? a : b;").is_empty());
    }


    #[test]
    fn allows_already_using_math_min() {
        assert!(run_on("const x = Math.min(a, b);").is_empty());
    }
}
