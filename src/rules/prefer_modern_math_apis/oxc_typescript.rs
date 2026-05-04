//! prefer-modern-math-apis OXC backend — flag legacy math expressions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

/// True if `expr` is `Math.<name>(...)`.
fn is_math_call<'a>(expr: &Expression<'a>, name: &str) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    is_math_member(&call.callee, name)
}

/// True if `expr` is `Math.<name>` (member expression).
fn is_math_member(expr: &Expression, name: &str) -> bool {
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name.as_str() == "Math" && member.property.name.as_str() == name
}

/// Unwrap parenthesized/TS assertion expressions.
fn unwrap<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(p) => unwrap(&p.expression),
        Expression::TSNonNullExpression(ts) => unwrap(&ts.expression),
        Expression::TSAsExpression(ts) => unwrap(&ts.expression),
        Expression::TSSatisfiesExpression(ts) => unwrap(&ts.expression),
        Expression::TSTypeAssertion(ts) => unwrap(&ts.expression),
        _ => expr,
    }
}

/// If the binary expression is one of the log-conversion shapes, return the suggestion.
fn log_violation_message<'a>(
    left: &'a Expression<'a>,
    right: &'a Expression<'a>,
    op: BinaryOperator,
) -> Option<&'static str> {
    let left = unwrap(left);
    let right = unwrap(right);

    match op {
        BinaryOperator::Division => {
            if !is_math_call(left, "log") {
                return None;
            }
            if is_math_member(right, "LN2") {
                Some("Prefer `Math.log2(x)` over `Math.log(x) / Math.LN2`.")
            } else if is_math_member(right, "LN10") {
                Some("Prefer `Math.log10(x)` over `Math.log(x) / Math.LN10`.")
            } else {
                None
            }
        }
        BinaryOperator::Multiplication => {
            let other = if is_math_call(left, "log") {
                right
            } else if is_math_call(right, "log") {
                left
            } else {
                return None;
            };
            if is_math_member(other, "LOG2E") {
                Some("Prefer `Math.log2(x)` over `Math.log(x) * Math.LOG2E`.")
            } else if is_math_member(other, "LOG10E") {
                Some("Prefer `Math.log10(x)` over `Math.log(x) * Math.LOG10E`.")
            } else {
                None
            }
        }
        _ => None,
    }
}

/// True if `expr` is `<expr> ** 2`.
fn is_squared<'a>(expr: &'a Expression<'a>, _source: &str) -> bool {
    let n = unwrap(expr);
    let Expression::BinaryExpression(bin) = n else {
        return false;
    };
    if bin.operator != BinaryOperator::Exponential {
        return false;
    }
    let r = unwrap(&bin.right);
    let Expression::NumericLiteral(num) = r else {
        return false;
    };
    num.value == 2.0
}

/// True if `expr` is `a * a` shape (same text on both sides).
fn is_self_mul<'a>(expr: &'a Expression<'a>, source: &str) -> bool {
    let n = unwrap(expr);
    let Expression::BinaryExpression(bin) = n else {
        return false;
    };
    if bin.operator != BinaryOperator::Multiplication {
        return false;
    }
    let lt = unwrap(&bin.left);
    let rt = unwrap(&bin.right);
    let left_text = &source[lt.span().start as usize..lt.span().end as usize];
    let right_text = &source[rt.span().start as usize..rt.span().end as usize];
    !left_text.is_empty() && left_text == right_text
}

/// Collect `+`-joined terms from a binary expression tree.
fn collect_plus_terms<'a, 'b>(
    expr: &'a Expression<'a>,
    out: &mut Vec<&'a Expression<'a>>,
) {
    let n = unwrap(expr);
    if let Expression::BinaryExpression(bin) = n
        && bin.operator == BinaryOperator::Addition {
            collect_plus_terms(&bin.left, out);
            collect_plus_terms(&bin.right, out);
            return;
        }
    out.push(n);
}

fn is_sum_of_squares<'a>(expr: &'a Expression<'a>, source: &str) -> bool {
    let mut terms = Vec::new();
    collect_plus_terms(expr, &mut terms);
    if terms.len() < 2 {
        return false;
    }
    terms.iter().all(|t| is_squared(t, source) || is_self_mul(t, source))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::BinaryExpression(bin) => {
                if let Some(msg) =
                    log_violation_message(&bin.left, &bin.right, bin.operator)
                {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, bin.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: msg.into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::CallExpression(call) => {
                if !is_math_member(&call.callee, "sqrt") {
                    return;
                }
                let Some(arg) = call.arguments.first() else {
                    return;
                };
                let arg_expr = match arg {
                    oxc_ast::ast::Argument::SpreadElement(_) => return,
                    _ => arg.to_expression(),
                };
                if is_sum_of_squares(arg_expr, ctx.source) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Prefer `Math.hypot(a, b)` over `Math.sqrt(a**2 + b**2)`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}
