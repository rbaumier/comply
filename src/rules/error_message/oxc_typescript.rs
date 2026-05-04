//! error-message OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const BUILTIN_ERRORS: &[&str] = &[
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "AggregateError",
    "SuppressedError",
];

fn message_arg_index(ctor_name: &str) -> usize {
    match ctor_name {
        "AggregateError" => 1,
        "SuppressedError" => 2,
        _ => 0,
    }
}

fn ctor_name_from_expr<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    if let Expression::Identifier(id) = expr {
        let name = id.name.as_str();
        if BUILTIN_ERRORS.contains(&name) {
            return Some(name);
        }
    }
    None
}

fn check_args(
    ctor_name: &str,
    args: &oxc_allocator::Vec<Argument>,
    node_span: oxc_span::Span,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let msg_index = message_arg_index(ctor_name);

    // Check for spread before message index
    for (i, arg) in args.iter().enumerate() {
        if i > msg_index {
            break;
        }
        if matches!(arg, Argument::SpreadElement(_)) {
            return;
        }
    }

    let msg_arg = args.get(msg_index);
    match msg_arg {
        None => {
            let (line, column) = byte_offset_to_line_col(ctx.source, node_span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Pass a message to the `{ctor_name}` constructor."),
                severity: Severity::Warning,
                span: None,
            });
        }
        Some(arg) => {
            let expr = match arg {
                Argument::SpreadElement(_) => return,
                _ => arg.as_expression().unwrap(),
            };
            let arg_span = expr.span();

            // Array or object literal
            if matches!(expr, Expression::ArrayExpression(_) | Expression::ObjectExpression(_)) {
                let (line, column) = byte_offset_to_line_col(ctx.source, arg_span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Error message should be a string.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }

            // Empty string literal
            if let Expression::StringLiteral(s) = expr
                && s.value.is_empty() {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, arg_span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Error message should not be an empty string.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }

            // Empty template literal
            if let Expression::TemplateLiteral(tpl) = expr
                && tpl.expressions.is_empty() && tpl.quasis.len() == 1
                    && let Some(q) = tpl.quasis.first()
                        && q.value.raw.is_empty() {
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, arg_span.start as usize);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column,
                                rule_id: super::META.id.into(),
                                message: "Error message should not be an empty string.".into(),
                                severity: Severity::Warning,
                                span: None,
                            });
                            return;
                        }

            // Numeric or boolean literal
            if matches!(
                expr,
                Expression::NumericLiteral(_) | Expression::BooleanLiteral(_)
            ) {
                let (line, column) = byte_offset_to_line_col(ctx.source, arg_span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Error message should be a string.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression, AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "Error",
            "EvalError",
            "RangeError",
            "ReferenceError",
            "SyntaxError",
            "TypeError",
            "URIError",
            "AggregateError",
            "SuppressedError",
        ])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::NewExpression(new_expr) => {
                let Some(ctor_name) = ctor_name_from_expr(&new_expr.callee) else {
                    return;
                };
                check_args(ctor_name, &new_expr.arguments, new_expr.span, ctx, diagnostics);
            }
            AstKind::CallExpression(call) => {
                let Some(ctor_name) = ctor_name_from_expr(&call.callee) else {
                    return;
                };
                check_args(ctor_name, &call.arguments, call.span, ctx, diagnostics);
            }
            _ => {}
        }
    }
}
