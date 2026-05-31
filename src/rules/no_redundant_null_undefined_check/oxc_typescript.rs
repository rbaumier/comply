use std::sync::Arc;

use oxc_ast::ast::{BinaryOperator, Expression, LogicalOperator};
use oxc_span::GetSpan;

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

#[derive(PartialEq, Clone, Copy)]
enum Nullish {
    Null,
    Undefined,
}

fn span_text(source: &str, span: oxc_span::Span) -> &str {
    &source[span.start as usize..span.end as usize]
}

/// If `expr` is the `null` literal or the `undefined` identifier, report which.
fn nullish_of(expr: &Expression) -> Option<Nullish> {
    match expr {
        Expression::NullLiteral(_) => Some(Nullish::Null),
        Expression::Identifier(id) if id.name.as_str() == "undefined" => Some(Nullish::Undefined),
        _ => None,
    }
}

/// Match one side of the logical expression: a `BinaryExpression` with the
/// expected operator comparing an operand against `null` or `undefined`.
/// Returns the operand's source text and which nullish value it was compared to.
fn nullish_comparison<'a>(
    expr: &'a Expression<'a>,
    source: &'a str,
    expected_op: BinaryOperator,
) -> Option<(&'a str, Nullish)> {
    let Expression::BinaryExpression(bin) = expr else {
        return None;
    };
    if bin.operator != expected_op {
        return None;
    }
    if let Some(kind) = nullish_of(&bin.right) {
        return Some((span_text(source, bin.left.span()).trim(), kind));
    }
    if let Some(kind) = nullish_of(&bin.left) {
        return Some((span_text(source, bin.right.span()).trim(), kind));
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["undefined"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LogicalExpression(logical) = node.kind() else {
            return;
        };

        // `&&` pairs with `!==`; `||` pairs with `===`. `??` never matches.
        let (expected_op, joiner, op_str, loose) = match logical.operator {
            LogicalOperator::And => (BinaryOperator::StrictInequality, "&&", "!==", "!="),
            LogicalOperator::Or => (BinaryOperator::StrictEquality, "||", "===", "=="),
            LogicalOperator::Coalesce => return,
        };

        let Some((left_op, left_kind)) = nullish_comparison(&logical.left, ctx.source, expected_op)
        else {
            return;
        };
        let Some((right_op, right_kind)) =
            nullish_comparison(&logical.right, ctx.source, expected_op)
        else {
            return;
        };

        // Need one `null` and one `undefined` comparison on the same operand.
        if left_kind == right_kind || left_op != right_op || left_op.is_empty() {
            return;
        }

        let guard = if expected_op == BinaryOperator::StrictInequality {
            format!("isDefined({left_op})")
        } else {
            format!("!isDefined({left_op})")
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, logical.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Redundant nullish check: `{left_op} {op_str} null {joiner} {left_op} {op_str} undefined` \
                 — replace with a single strict, type-narrowing guard like `{guard}` \
                 (a `value is NonNullable<T>` helper), not `{left_op} {loose} null`."
            ),
            severity: super::META.severity,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_and_neq_null_undefined() {
        let d = run_on("if (x !== null && x !== undefined) {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-redundant-null-undefined-check");
    }

    #[test]
    fn flags_or_eq_null_undefined() {
        assert_eq!(run_on("if (x === null || x === undefined) {}").len(), 1);
    }

    #[test]
    fn flags_reversed_undefined_first() {
        assert_eq!(run_on("if (x !== undefined && x !== null) {}").len(), 1);
    }

    #[test]
    fn flags_member_operand() {
        assert_eq!(
            run_on("if (obj.prop !== null && obj.prop !== undefined) {}").len(),
            1
        );
    }

    #[test]
    fn flags_literal_on_left() {
        assert_eq!(run_on("if (null !== x && undefined !== x) {}").len(), 1);
    }

    #[test]
    fn allows_different_operands() {
        assert!(run_on("if (x !== null && y !== undefined) {}").is_empty());
    }

    #[test]
    fn allows_single_null_check() {
        assert!(run_on("if (x !== null) {}").is_empty());
    }

    #[test]
    fn allows_double_null() {
        // Same operand but both `null` — a different, out-of-scope redundancy.
        assert!(run_on("if (x !== null && x !== null) {}").is_empty());
    }

    #[test]
    fn allows_mismatched_operators() {
        // `&&` with a `===` side is not the redundant pattern.
        assert!(run_on("if (x !== null && x === undefined) {}").is_empty());
    }

    #[test]
    fn allows_and_with_strict_equality() {
        // `&&` pairs with `!==`; `x === null && x === undefined` is always false,
        // a contradiction that is out of this rule's scope.
        assert!(run_on("if (x === null && x === undefined) {}").is_empty());
    }

    #[test]
    fn allows_loose_equality() {
        // Loose `!=` already covers both; this rule scopes to strict equality.
        assert!(run_on("if (x != null && x != undefined) {}").is_empty());
    }

    #[test]
    fn allows_nullish_coalescing() {
        assert!(run_on("const y = x ?? undefined;").is_empty());
    }

    // Regression for #737 — composition.ts:40 from the amadeo deslop run.
    #[test]
    fn flags_response_value_check() {
        let d = run_on("const ok = responseValue !== null && responseValue !== undefined;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("responseValue"));
    }
}
