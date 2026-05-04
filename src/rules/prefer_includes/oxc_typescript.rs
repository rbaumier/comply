//! OxcCheck backend for prefer-includes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["indexOf"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };
        let op = bin.operator.as_str();

        // Try call on left, literal on right.
        let (call_span, lhs_call) = if is_indexof_call(&bin.left, ctx.source) {
            let Some(lit) = literal_text(&bin.right, ctx.source) else { return };
            if !is_existence_check(op, &lit, true) { return; }
            (indexof_call_span(&bin.left), true)
        } else if is_indexof_call(&bin.right, ctx.source) {
            let Some(lit) = literal_text(&bin.left, ctx.source) else { return };
            if !is_existence_check(op, &lit, false) { return; }
            (indexof_call_span(&bin.right), false)
        } else {
            return;
        };

        let _ = lhs_call;
        let (line, column) = byte_offset_to_line_col(ctx.source, call_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `.includes(x)` over `.indexOf(x) !== -1` — more readable.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_indexof_call(expr: &Expression, _source: &str) -> bool {
    let call = unwrap_expr_to_call(expr);
    let Some(call) = call else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    let name = member.property.name.as_str();
    name == "indexOf" || name == "lastIndexOf"
}

fn indexof_call_span(expr: &Expression) -> oxc_span::Span {
    use oxc_span::GetSpan;
    let call = unwrap_expr_to_call(expr);
    call.map(|c| c.span).unwrap_or_else(|| expr.span())
}

fn unwrap_expr_to_call<'a>(expr: &'a Expression<'a>) -> Option<&'a CallExpression<'a>> {
    let inner = unwrap_expr(expr);
    match inner {
        Expression::CallExpression(call) => Some(call),
        _ => None,
    }
}

fn unwrap_expr<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(p) => unwrap_expr(&p.expression),
        Expression::TSNonNullExpression(n) => unwrap_expr(&n.expression),
        Expression::TSAsExpression(a) => unwrap_expr(&a.expression),
        Expression::TSSatisfiesExpression(s) => unwrap_expr(&s.expression),
        Expression::TSTypeAssertion(t) => unwrap_expr(&t.expression),
        other => other,
    }
}

fn literal_text(expr: &Expression, _source: &str) -> Option<String> {
    let inner = unwrap_expr(expr);
    match inner {
        Expression::NumericLiteral(n) => {
            let v = n.value;
            if v == 0.0 {
                Some("0".to_string())
            } else if v == -1.0 {
                Some("-1".to_string())
            } else {
                None
            }
        }
        Expression::UnaryExpression(u) => {
            if u.operator == UnaryOperator::UnaryNegation
                && let Expression::NumericLiteral(n) = &u.argument
                    && n.value == 1.0 {
                        return Some("-1".to_string());
                    }
            None
        }
        _ => None,
    }
}

fn is_existence_check(op: &str, lit: &str, lhs_call: bool) -> bool {
    matches!(
        (op, lit, lhs_call),
        ("!==", "-1", _)
            | ("!=", "-1", _)
            | ("===", "-1", _)
            | ("==", "-1", _)
            | (">", "-1", true)
            | ("<", "-1", false)
            | (">=", "0", true)
            | ("<=", "0", false)
            | ("<", "0", true)
            | (">", "0", false)
    )
}
