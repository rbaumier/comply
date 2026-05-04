use std::sync::Arc;

use oxc_ast::ast::{BinaryOperator, Expression, LogicalOperator};
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

/// Check if `expr` is `IDENT.length`. Returns the source text of the
/// object if matched.
fn is_length_access<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    let Expression::StaticMemberExpression(member) = expr else {
        return None;
    };
    if member.property.name.as_str() != "length" {
        return None;
    }
    if let Expression::Identifier(obj) = &member.object {
        return Some(obj.name.as_str());
    }
    None
}

/// Check if `expr` is `IDENT.length > 0`, `IDENT.length !== 0`, or
/// `IDENT.length === 0`. Returns (identifier_name, is_non_zero_check).
fn is_length_compare_zero<'a>(expr: &'a Expression<'a>) -> Option<(&'a str, bool)> {
    let Expression::BinaryExpression(bin) = expr else {
        return None;
    };
    // Right must be `0`.
    let Expression::NumericLiteral(num) = &bin.right else {
        return None;
    };
    if num.value != 0.0 {
        return None;
    }
    let name = is_length_access(&bin.left)?;
    match bin.operator {
        BinaryOperator::GreaterThan | BinaryOperator::StrictInequality => Some((name, true)),
        BinaryOperator::StrictEquality => Some((name, false)),
        _ => None,
    }
}

/// Check if `expr` is `IDENT.some(...)` or `IDENT.every(...)`.
/// Returns (identifier_name, method_name).
fn is_some_or_every_call<'a>(expr: &'a Expression<'a>) -> Option<(&'a str, &'a str)> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let method = member.property.name.as_str();
    if method != "some" && method != "every" {
        return None;
    }
    let Expression::Identifier(obj) = &member.object else {
        return None;
    };
    Some((obj.name.as_str(), method))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["length"])
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

        // Pattern 1: `arr.length > 0 && arr.some(fn)` or `arr.length !== 0 && arr.some(fn)`
        if logical.operator == LogicalOperator::And {
            if let Some((len_name, true)) = is_length_compare_zero(&logical.left) {
                if let Some((call_name, "some")) = is_some_or_every_call(&logical.right) {
                    if len_name == call_name {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, logical.left.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "The non-empty check is useless as `Array#some()` returns `false` for an empty array.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                        return;
                    }
                }
            }
        }

        // Pattern 2: `arr.length === 0 || arr.every(fn)`
        if logical.operator == LogicalOperator::Or {
            if let Some((len_name, false)) = is_length_compare_zero(&logical.left) {
                if let Some((call_name, "every")) = is_some_or_every_call(&logical.right) {
                    if len_name == call_name {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, logical.left.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "The empty check is useless as `Array#every()` returns `true` for an empty array.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
        }
    }
}
