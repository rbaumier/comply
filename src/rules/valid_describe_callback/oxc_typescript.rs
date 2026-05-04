//! OXC backend for valid-describe-callback.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Check if a call expression's callee is `describe` (bare) or
/// `describe.skip` / `describe.only` / `describe.each(...)`.
fn is_describe_callee(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => id.name.as_str() == "describe",
        Expression::StaticMemberExpression(member) => {
            is_describe_callee(&member.object)
        }
        Expression::CallExpression(call) => {
            is_describe_callee(&call.callee)
        }
        _ => false,
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

        if !is_describe_callee(&call.callee) {
            return;
        }

        // The callback is the second argument
        if call.arguments.len() < 2 {
            return;
        }
        let cb = &call.arguments[1];

        match cb {
            oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) => {
                let is_async = arrow.r#async;
                let has_params = !arrow.params.items.is_empty();
                let returns_value = if arrow.expression {
                    // Arrow with expression body = implicit return
                    true
                } else {
                    body_returns_value_stmts(&arrow.body.statements)
                };

                let message = if is_async {
                    "`describe` callback must not be async."
                } else if has_params {
                    "`describe` callback must not declare parameters."
                } else if returns_value {
                    "`describe` callback must not return a value."
                } else {
                    return;
                };

                let (line, column) = byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "valid-describe-callback".into(),
                    message: message.into(),
                    severity: Severity::Warning,
                    span: Some((arrow.span.start as usize, (arrow.span.end - arrow.span.start) as usize)),
                });
            }
            oxc_ast::ast::Argument::FunctionExpression(func) => {
                let is_async = func.r#async;
                let has_params = !func.params.items.is_empty();
                let returns_value = func.body.as_ref()
                    .map(|body| body_returns_value_stmts(&body.statements))
                    .unwrap_or(false);

                let message = if is_async {
                    "`describe` callback must not be async."
                } else if has_params {
                    "`describe` callback must not declare parameters."
                } else if returns_value {
                    "`describe` callback must not return a value."
                } else {
                    return;
                };

                let (line, column) = byte_offset_to_line_col(ctx.source, func.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "valid-describe-callback".into(),
                    message: message.into(),
                    severity: Severity::Warning,
                    span: Some((func.span.start as usize, (func.span.end - func.span.start) as usize)),
                });
            }
            _ => {}
        }
    }
}

/// Walk statements looking for a `return` with a value, without descending
/// into nested functions.
fn body_returns_value_stmts(stmts: &[oxc_ast::ast::Statement]) -> bool {
    use oxc_ast::ast::Statement;
    for stmt in stmts.iter() {
        match stmt {
            Statement::ReturnStatement(ret) => {
                if ret.argument.is_some() {
                    return true;
                }
            }
            Statement::BlockStatement(block) => {
                if body_returns_value_stmts(&block.body) {
                    return true;
                }
            }
            Statement::IfStatement(if_stmt) => {
                if stmt_returns_value(&if_stmt.consequent) {
                    return true;
                }
                if let Some(ref alt) = if_stmt.alternate
                    && stmt_returns_value(alt) {
                        return true;
                    }
            }
            // Don't descend into nested function declarations/expressions
            Statement::FunctionDeclaration(_) => continue,
            Statement::ExpressionStatement(expr_stmt) => {
                // Skip function expressions / arrow functions at statement level
                match &expr_stmt.expression {
                    Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => continue,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    false
}

fn stmt_returns_value(stmt: &oxc_ast::ast::Statement) -> bool {
    use oxc_ast::ast::Statement;
    match stmt {
        Statement::ReturnStatement(ret) => ret.argument.is_some(),
        Statement::BlockStatement(block) => body_returns_value_stmts(&block.body),
        Statement::IfStatement(if_stmt) => {
            stmt_returns_value(&if_stmt.consequent)
                || if_stmt.alternate.as_ref().is_some_and(|alt| stmt_returns_value(alt))
        }
        Statement::FunctionDeclaration(_) => false,
        _ => false,
    }
}
