//! prefer-mock-promise-shorthand OxcCheck backend — flag
//! `x.mockImplementation(() => Promise.resolve(v))` and
//! `x.mockImplementation(() => Promise.reject(v))`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["mockImplementation"])
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

        // Callee must be a member expression ending in `.mockImplementation`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "mockImplementation" {
            return;
        }

        // Exactly one argument: a function
        if call.arguments.len() != 1 {
            return;
        }
        let Some(arg) = call.arguments.first() else {
            return;
        };

        let kind = match arg {
            Argument::ArrowFunctionExpression(arrow) => settle_kind_from_arrow(arrow, ctx.source),
            Argument::FunctionExpression(func) => settle_kind_from_func(func, ctx.source),
            _ => None,
        };

        let Some(kind) = kind else {
            return;
        };

        let shorthand = match kind {
            "resolve" => "mockResolvedValue",
            "reject" => "mockRejectedValue",
            _ => return,
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `.{shorthand}(x)` over `.mockImplementation(() => Promise.{kind}(x))`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// If `body` is a `Promise.resolve(x)` / `Promise.reject(x)` call expression,
/// return the property name (`"resolve"` or `"reject"`).
fn promise_settle_kind<'a>(expr: &Expression<'a>) -> Option<&'static str> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let Expression::Identifier(obj) = &member.object else {
        return None;
    };
    if obj.name.as_str() != "Promise" {
        return None;
    }
    match member.property.name.as_str() {
        "resolve" => Some("resolve"),
        "reject" => Some("reject"),
        _ => None,
    }
}

fn settle_kind_from_arrow<'a>(
    arrow: &oxc_ast::ast::ArrowFunctionExpression<'a>,
    _source: &str,
) -> Option<&'static str> {
    // If expression body (single expression, no braces)
    if arrow.expression {
        let stmts = &arrow.body.statements;
        if stmts.len() == 1
            && let Statement::ExpressionStatement(expr_stmt) = &stmts[0] {
                return promise_settle_kind(&expr_stmt.expression);
            }
        return None;
    }

    // Block body: must contain exactly one return statement
    settle_kind_from_block_body(&arrow.body.statements)
}

fn settle_kind_from_func<'a>(
    func: &oxc_ast::ast::Function<'a>,
    _source: &str,
) -> Option<&'static str> {
    let body = func.body.as_ref()?;
    settle_kind_from_block_body(&body.statements)
}

fn settle_kind_from_block_body<'a>(
    stmts: &[Statement<'a>],
) -> Option<&'static str> {
    // Must contain exactly one non-empty statement, which is a return statement
    let mut return_stmt = None;
    for stmt in stmts {
        match stmt {
            Statement::EmptyStatement(_) => continue,
            Statement::ReturnStatement(ret) => {
                if return_stmt.is_some() {
                    return None; // More than one statement
                }
                return_stmt = Some(ret);
            }
            _ => return None, // Non-return statement
        }
    }
    let ret = return_stmt?;
    let expr = ret.argument.as_ref()?;
    promise_settle_kind(expr)
}
