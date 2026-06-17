//! OXC backend for prefer-math-trunc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Signed 32-bit integer bounds. `| 0` (ToInt32) wraps any value outside this
/// range; `Math.trunc` does not — so the two diverge there.
const INT32_MIN: f64 = i32::MIN as f64;
const INT32_MAX: f64 = i32::MAX as f64;

fn is_zero_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::NumericLiteral(lit) if lit.value == 0.0)
}

/// A numeric literal with no fractional part (`1`, `5`, `0b10`). There is
/// nothing to truncate, so `x | 0` on it is flag composition, not the
/// `Math.trunc` idiom. A fractional literal (`2.5`) returns false.
fn is_integer_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::NumericLiteral(lit) if lit.value.fract() == 0.0)
}

/// A numeric literal (optionally signed) whose value falls outside the signed
/// int32 range, so `| 0` (ToInt32) changes it while `Math.trunc` would not.
fn is_out_of_int32_range_literal(expr: &Expression) -> bool {
    match expr.get_inner_expression() {
        Expression::NumericLiteral(lit) => lit.value < INT32_MIN || lit.value > INT32_MAX,
        // `-0xffffffff`: a leading `-`/`+` on a numeric literal.
        Expression::UnaryExpression(unary)
            if matches!(
                unary.operator,
                UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus
            ) =>
        {
            matches!(
                unary.argument.get_inner_expression(),
                Expression::NumericLiteral(lit) if lit.value < INT32_MIN || lit.value > INT32_MAX
            )
        }
        _ => false,
    }
}

fn is_bitwise_operator(op: BinaryOperator) -> bool {
    matches!(
        op,
        BinaryOperator::BitwiseOR
            | BinaryOperator::BitwiseAnd
            | BinaryOperator::BitwiseXOR
            | BinaryOperator::ShiftLeft
            | BinaryOperator::ShiftRight
            | BinaryOperator::ShiftRightZeroFill
    )
}

/// `| 0` on this left operand is a ToInt32 / 32-bit coercion, not a fractional
/// truncation, so suggesting `Math.trunc` would change behavior.
fn is_int32_coercion_left(left: &Expression) -> bool {
    let inner = left.get_inner_expression();
    match inner {
        // Result of a bitwise op is already int32; `| 0` is an idempotent
        // ToInt32 normalization (e.g. `1 << layer | 0`).
        Expression::BinaryExpression(bin) if is_bitwise_operator(bin.operator) => true,
        // `~expr` is already int32.
        Expression::UnaryExpression(unary)
            if unary.operator == UnaryOperator::BitwiseNot =>
        {
            true
        }
        // Multiplication where an operand is an out-of-int32-range literal: the
        // product can exceed int32, so `| 0` is a wrap (e.g. `Math.random() * 0xffffffff`).
        Expression::BinaryExpression(bin)
            if bin.operator == BinaryOperator::Multiplication
                && (is_out_of_int32_range_literal(&bin.left)
                    || is_out_of_int32_range_literal(&bin.right)) =>
        {
            true
        }
        // A numeric literal (optionally signed) outside int32 range (e.g.
        // `0xffffffff`); `| 0` wraps it.
        _ => is_out_of_int32_range_literal(inner),
    }
}

/// True when `node` sits inside a `TSEnumMember` initializer, a
/// constant-expression position where TS forbids function calls (TS2474). The
/// `Math.trunc(x)` remediation is uncompilable there, so the rule must not
/// suggest it.
fn is_in_enum_member_initializer<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    semantic
        .nodes()
        .ancestors(node.id())
        .any(|ancestor| matches!(ancestor.kind(), AstKind::TSEnumMember(_)))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::UnaryExpression,
            AstType::BinaryExpression,
            AstType::AssignmentExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Enum member initializers are constant-expression positions where TS
        // forbids function calls (TS2474), so `Math.trunc(x)` is uncompilable
        // there — never suggest it for a `BinaryExpression`/`AssignmentExpression`
        // inside one.
        if is_in_enum_member_initializer(node, semantic) {
            return;
        }
        match node.kind() {
            AstKind::UnaryExpression(unary) => {
                // ~~x: outer ~ whose argument is also ~expr
                if unary.operator != UnaryOperator::BitwiseNot {
                    return;
                }
                let Expression::UnaryExpression(inner) = &unary.argument else {
                    return;
                };
                if inner.operator != UnaryOperator::BitwiseNot {
                    return;
                }
                // Don't double-fire: skip if our parent is also `~`
                let parent = semantic.nodes().parent_node(node.id());
                if let AstKind::UnaryExpression(p) = parent.kind()
                    && p.operator == UnaryOperator::BitwiseNot {
                        return;
                    }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, unary.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Use `Math.trunc(x)` instead of `~~x`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::BinaryExpression(bin) => {
                let op = bin.operator;
                if !matches!(
                    op,
                    BinaryOperator::BitwiseOR
                        | BinaryOperator::ShiftRight
                        | BinaryOperator::ShiftLeft
                        | BinaryOperator::BitwiseXOR
                ) {
                    return;
                }
                if !is_zero_literal(&bin.right) {
                    return;
                }
                // An integer-literal left operand (`1 << 0`, `5 | 0`) has no
                // fractional part to truncate, so this is flag composition, not
                // the `Math.trunc` idiom. A fractional literal (`2.5 | 0`) still
                // flags.
                if is_integer_literal(bin.left.get_inner_expression()) {
                    return;
                }
                // `| 0` used as a ToInt32 / 32-bit coercion (not fractional
                // truncation) diverges from `Math.trunc`; don't suggest it.
                if is_int32_coercion_left(&bin.left) {
                    return;
                }
                let op_str = &ctx.source[bin.left.span().end as usize..bin.right.span().start as usize].trim();
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, bin.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Use `Math.trunc(x)` instead of bitwise `{op_str} 0`."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::AssignmentExpression(assign) => {
                use oxc_ast::ast::AssignmentOperator;
                if !matches!(
                    assign.operator,
                    AssignmentOperator::BitwiseOR
                        | AssignmentOperator::ShiftRight
                        | AssignmentOperator::ShiftLeft
                        | AssignmentOperator::BitwiseXOR
                ) {
                    return;
                }
                if !is_zero_literal(&assign.right) {
                    return;
                }
                let op_str = match assign.operator {
                    AssignmentOperator::BitwiseOR => "|=",
                    AssignmentOperator::ShiftRight => ">>=",
                    AssignmentOperator::ShiftLeft => "<<=",
                    AssignmentOperator::BitwiseXOR => "^=",
                    _ => return,
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Use `Math.trunc(x)` instead of bitwise assignment `{op_str} 0`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // True positives: genuine fractional-truncation idioms must still fire.

    #[test]
    fn flags_identifier_or_zero() {
        assert_eq!(run("const n = value | 0;").len(), 1);
    }

    #[test]
    fn flags_small_literal_or_zero() {
        assert_eq!(run("const n = 2.5 | 0;").len(), 1);
    }

    #[test]
    fn flags_multiplication_small_literal() {
        assert_eq!(run("const n = x * 2 | 0;").len(), 1);
    }

    #[test]
    fn flags_price_or_zero() {
        assert_eq!(run("const n = price | 0;").len(), 1);
    }

    #[test]
    fn flags_double_tilde() {
        assert_eq!(run("const n = ~~value;").len(), 1);
    }

    // False positives: an integer-literal left operand has nothing to truncate,
    // and `| 0` used as a ToInt32 / 32-bit coercion must be exempt.

    #[test]
    fn allows_int32_max_literal() {
        // An integer literal has no fractional part to truncate; `| 0` is a no-op.
        assert!(run("const n = 2147483647 | 0;").is_empty());
    }

    #[test]
    fn allows_integer_literal_or_zero() {
        // `5 | 0` is flag composition between constants, not float truncation.
        assert!(run("const n = 5 | 0;").is_empty());
    }

    #[test]
    fn allows_power_of_two_flag() {
        // `1 << 0` is the canonical power-of-two flag idiom, not `x << 0` truncation.
        assert!(run("const x = 1 << 0;").is_empty());
    }

    #[test]
    fn allows_out_of_range_literal() {
        // 0xffffffff = 4294967295 > int32 max; `| 0` === -1, diverges from Math.trunc.
        assert!(run("const mask = 0xffffffff | 0;").is_empty());
    }

    #[test]
    fn allows_int32_min_minus_one() {
        // -2147483649 is below int32 min, so `| 0` wraps.
        assert!(run("const n = -2147483649 | 0;").is_empty());
    }

    #[test]
    fn allows_bitwise_left_operand() {
        assert!(run("const m = 1 << layer | 0;").is_empty());
    }

    #[test]
    fn allows_bitwise_and_left_operand() {
        assert!(run("const m = mask & 0xff | 0;").is_empty());
    }

    #[test]
    fn allows_unary_tilde_left_operand() {
        assert!(run("const n = ~x | 0;").is_empty());
    }

    #[test]
    fn allows_multiplication_out_of_range_literal() {
        assert!(run("const d0 = Math.random() * 0xffffffff | 0;").is_empty());
    }

    #[test]
    fn allows_paren_bitwise_left_operand() {
        assert!(run("const m = (1 << layer) | 0;").is_empty());
    }

    // Unchanged behavior.

    #[test]
    fn allows_math_trunc() {
        assert!(run("const n = Math.trunc(value);").is_empty());
    }

    #[test]
    fn ignores_string_literal() {
        assert!(run(r#"const s = "value | 0";"#).is_empty());
    }

    // Enum member initializers are constant-expression positions where TS
    // forbids `Math.trunc(...)` (TS2474), so the rule must not fire there.

    #[test]
    fn allows_const_enum_identifier_or_zero() {
        // babel scopeflags.ts: `KIND_VALUE | 0` is bitflag composition between
        // constants, used as a column-alignment no-op.
        assert!(run("const enum E { A = 1, B = 4, C = A | 0 }").is_empty());
    }

    #[test]
    fn allows_const_enum_shift_flags() {
        // babel scope.ts: `1 << 0` power-of-two flag in a const enum.
        assert!(run("const enum N { Var = 1 << 0, Lexical = 1 << 1 }").is_empty());
    }

    #[test]
    fn allows_plain_enum_member_initializer() {
        // TS2474 forbids function calls in any enum member initializer, const or
        // not, so a runtime-looking `x | 0` is still uncompilable as Math.trunc.
        assert!(run("enum E { A = x | 0 }").is_empty());
    }

    // True positives: genuine fractional-truncation idioms outside enums must
    // still fire.

    #[test]
    fn flags_runtime_identifier_outside_enum() {
        assert_eq!(run("const y = f | 0;").len(), 1);
    }

    #[test]
    fn flags_runtime_assignment_outside_enum() {
        assert_eq!(run("value = value | 0;").len(), 1);
    }
}
