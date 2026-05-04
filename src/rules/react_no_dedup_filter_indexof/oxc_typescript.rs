//! react-no-dedup-filter-indexof oxc backend.
//!
//! Matches `foo.filter(...)` whose callback body contains a `.indexOf(` call.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

/// Check if an expression tree contains a `.indexOf(...)` call.
fn contains_indexof(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(member) = &call.callee
                && member.property.name.as_str() == "indexOf" {
                    return true;
                }
            // Recurse into callee and arguments.
            if contains_indexof(&call.callee) {
                return true;
            }
            for arg in &call.arguments {
                if contains_indexof_in_arg(arg) {
                    return true;
                }
            }
            false
        }
        Expression::StaticMemberExpression(member) => contains_indexof(&member.object),
        Expression::ComputedMemberExpression(member) => {
            contains_indexof(&member.object) || contains_indexof(&member.expression)
        }
        Expression::BinaryExpression(bin) => {
            contains_indexof(&bin.left) || contains_indexof(&bin.right)
        }
        Expression::LogicalExpression(log) => {
            contains_indexof(&log.left) || contains_indexof(&log.right)
        }
        Expression::UnaryExpression(un) => contains_indexof(&un.argument),
        Expression::ConditionalExpression(cond) => {
            contains_indexof(&cond.test)
                || contains_indexof(&cond.consequent)
                || contains_indexof(&cond.alternate)
        }
        Expression::ParenthesizedExpression(paren) => contains_indexof(&paren.expression),
        _ => false,
    }
}

fn contains_indexof_in_arg(arg: &Argument) -> bool {
    match arg {
        Argument::SpreadElement(spread) => contains_indexof(&spread.argument),
        _ => {
            if let Some(expr) = arg.as_expression() {
                contains_indexof(expr)
            } else {
                false
            }
        }
    }
}

fn callback_body_has_indexof(expr: &Expression) -> bool {
    match expr {
        Expression::ArrowFunctionExpression(arrow) => {
            // Concise body (expression).
            if arrow.expression
                && let Some(stmt) = arrow.body.statements.first()
                    && let oxc_ast::ast::Statement::ExpressionStatement(es) = stmt {
                        return contains_indexof(&es.expression);
                    }
            // Block body — check all statements.
            for stmt in &arrow.body.statements {
                if stmt_contains_indexof(stmt) {
                    return true;
                }
            }
            false
        }
        Expression::FunctionExpression(func) => {
            if let Some(body) = &func.body {
                for stmt in &body.statements {
                    if stmt_contains_indexof(stmt) {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

fn stmt_contains_indexof(stmt: &oxc_ast::ast::Statement) -> bool {
    match stmt {
        oxc_ast::ast::Statement::ExpressionStatement(es) => contains_indexof(&es.expression),
        oxc_ast::ast::Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                contains_indexof(arg)
            } else {
                false
            }
        }
        oxc_ast::ast::Statement::IfStatement(if_stmt) => {
            contains_indexof(&if_stmt.test)
                || stmt_contains_indexof(&if_stmt.consequent)
                || if_stmt
                    .alternate
                    .as_ref()
                    .is_some_and(|alt| stmt_contains_indexof(alt))
        }
        oxc_ast::ast::Statement::BlockStatement(block) => {
            block.body.iter().any(|s| stmt_contains_indexof(s))
        }
        oxc_ast::ast::Statement::VariableDeclaration(decl) => {
            decl.declarations.iter().any(|d| {
                d.init
                    .as_ref()
                    .is_some_and(|init| contains_indexof(init))
            })
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["filter"])
    }

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

        // Callee must be `<expr>.filter`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "filter" {
            return;
        }

        // Find arrow_function or function_expression in arguments.
        let Some(cb) = call.arguments.iter().find_map(|arg| {
            let expr = arg.as_expression()?;
            match expr {
                Expression::ArrowFunctionExpression(_)
                | Expression::FunctionExpression(_) => Some(expr),
                _ => None,
            }
        }) else {
            return;
        };

        if !callback_body_has_indexof(cb) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.filter(... indexOf ...)` is O(n²) dedup — use `[...new Set(arr)]` (O(n))."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
