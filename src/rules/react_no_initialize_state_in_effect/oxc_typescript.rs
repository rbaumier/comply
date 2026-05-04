//! react-no-initialize-state-in-effect OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

/// True if a call expression is a setter like `setFoo(...)`.
fn is_setter_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    let name = callee.name.as_str();
    name.starts_with("set")
        && name.len() > 3
        && name.as_bytes()[3].is_ascii_uppercase()
}

/// Walk statements recursively (but not into nested functions) looking for
/// a setter call.
fn body_calls_setter(stmts: &[oxc_ast::ast::Statement]) -> bool {
    for stmt in stmts {
        if stmt_calls_setter(stmt) {
            return true;
        }
    }
    false
}

fn stmt_calls_setter(stmt: &oxc_ast::ast::Statement) -> bool {
    match stmt {
        oxc_ast::ast::Statement::ExpressionStatement(expr) => {
            expr_calls_setter(&expr.expression)
        }
        oxc_ast::ast::Statement::VariableDeclaration(decl) => {
            decl.declarations.iter().any(|d| {
                d.init.as_ref().is_some_and(|e| expr_calls_setter(e))
            })
        }
        oxc_ast::ast::Statement::IfStatement(if_stmt) => {
            if let oxc_ast::ast::Statement::BlockStatement(block) = &if_stmt.consequent {
                if body_calls_setter(&block.body) {
                    return true;
                }
            }
            if let Some(alt) = &if_stmt.alternate {
                if stmt_calls_setter(alt) {
                    return true;
                }
            }
            false
        }
        oxc_ast::ast::Statement::BlockStatement(block) => body_calls_setter(&block.body),
        oxc_ast::ast::Statement::ReturnStatement(ret) => {
            ret.argument.as_ref().is_some_and(|e| expr_calls_setter(e))
        }
        _ => false,
    }
}

fn expr_calls_setter(expr: &Expression) -> bool {
    if is_setter_call(expr) {
        return true;
    }
    match expr {
        Expression::SequenceExpression(seq) => {
            seq.expressions.iter().any(|e| expr_calls_setter(e))
        }
        Expression::ConditionalExpression(cond) => {
            expr_calls_setter(&cond.consequent) || expr_calls_setter(&cond.alternate)
        }
        // Don't descend into nested functions.
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => false,
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useEffect"])
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
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name != "useEffect" {
            return;
        }
        if call.arguments.len() != 2 {
            return;
        }

        // Second arg must be an empty array.
        let deps_expr = call.arguments[1].to_expression();
        let Expression::ArrayExpression(deps_arr) = deps_expr else {
            return;
        };
        if !deps_arr.elements.is_empty() {
            return;
        }

        // First arg must be arrow/function with a body that calls a setter.
        let callback_expr = call.arguments[0].to_expression();
        let has_setter = match callback_expr {
            Expression::ArrowFunctionExpression(arrow) => {
                body_calls_setter(&arrow.body.statements)
            }
            Expression::FunctionExpression(func) => {
                func.body.as_ref().is_some_and(|b| body_calls_setter(&b.statements))
            }
            _ => return,
        };

        if !has_setter {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`useEffect` with empty deps sets state — initialize it in `useState(...)` directly instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
