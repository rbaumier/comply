//! better-result-catch-returns-tagged OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, Expression, FunctionBody, ObjectPropertyKind, PropertyKey, Statement,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Check whether a returned expression is a tagged error (new XxxError(...) where Xxx != Error).
fn is_tagged_error(expr: &Expression) -> bool {
    if let Expression::NewExpression(new_expr) = expr
        && let Expression::Identifier(id) = &new_expr.callee {
            let name = id.name.as_str();
            return name != "Error" && name.ends_with("Error");
        }
    false
}

/// Find the return expression from a function body.
fn find_return_expr_in_body<'a>(body: &'a FunctionBody<'a>) -> Option<&'a Expression<'a>> {
    for stmt in &body.statements {
        if let Some(expr) = find_return_expr_in_stmt(stmt) {
            return Some(expr);
        }
    }
    None
}

fn find_return_expr_in_stmt<'a>(stmt: &'a Statement<'a>) -> Option<&'a Expression<'a>> {
    match stmt {
        Statement::ReturnStatement(ret) => ret.argument.as_ref(),
        Statement::BlockStatement(block) => {
            for s in &block.body {
                if let Some(expr) = find_return_expr_in_stmt(s) {
                    return Some(expr);
                }
            }
            None
        }
        Statement::IfStatement(if_stmt) => {
            if let Some(expr) = find_return_expr_in_stmt(&if_stmt.consequent) {
                return Some(expr);
            }
            if let Some(alt) = &if_stmt.alternate {
                return find_return_expr_in_stmt(alt);
            }
            None
        }
        _ => None,
    }
}

/// Get the return expression from a handler (arrow or function expression).
fn handler_return_expr<'a>(value: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    match value {
        Expression::ArrowFunctionExpression(arrow) => {
            if arrow.expression {
                // concise body — the single expression IS the return value
                arrow.body.statements.first().and_then(|s| {
                    if let Statement::ExpressionStatement(e) = s {
                        Some(&e.expression)
                    } else {
                        None
                    }
                })
            } else {
                find_return_expr_in_body(&arrow.body)
            }
        }
        Expression::FunctionExpression(func) => {
            func.body.as_ref().and_then(|b| find_return_expr_in_body(b))
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // Check callee is Result.try or Result.tryPromise
        if let Expression::StaticMemberExpression(member) = &call.callee {
            if let Expression::Identifier(obj) = &member.object {
                if obj.name.as_str() != "Result" {
                    return;
                }
                let prop = member.property.name.as_str();
                if prop != "try" && prop != "tryPromise" {
                    return;
                }
            } else {
                return;
            }
        } else {
            return;
        }

        // Find the object argument { try: ..., catch: ... }
        for arg in &call.arguments {
            let Argument::ObjectExpression(obj) = arg else {
                continue;
            };
            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(p) = prop else {
                    continue;
                };
                let PropertyKey::StaticIdentifier(key) = &p.key else {
                    continue;
                };
                if key.name.as_str() != "catch" {
                    continue;
                }
                let Some(returned) = handler_return_expr(&p.value) else {
                    continue;
                };
                if is_tagged_error(returned) {
                    continue;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, p.value.span().start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message:
                        "catch handler should return a TaggedError, not a raw Error/string/object."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
