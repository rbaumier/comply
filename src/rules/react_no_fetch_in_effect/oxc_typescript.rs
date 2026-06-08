//! react-no-fetch-in-effect OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

fn is_effect_callee(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => {
            id.name == "useEffect" || id.name == "useLayoutEffect"
        }
        Expression::StaticMemberExpression(mem) => {
            if let Expression::Identifier(obj) = &mem.object {
                obj.name == "React"
                    && (mem.property.name == "useEffect"
                        || mem.property.name == "useLayoutEffect")
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Returns true if `fetch(...)` is reachable from the statements without
/// crossing a nested function boundary.
fn contains_top_level_fetch_stmts(stmts: &[oxc_ast::ast::Statement]) -> bool {
    stmts.iter().any(|s| stmt_has_fetch(s))
}

fn stmt_has_fetch(stmt: &oxc_ast::ast::Statement) -> bool {
    match stmt {
        oxc_ast::ast::Statement::ExpressionStatement(expr) => {
            expr_has_fetch(&expr.expression)
        }
        oxc_ast::ast::Statement::VariableDeclaration(decl) => {
            decl.declarations.iter().any(|d| {
                d.init.as_ref().is_some_and(|e| expr_has_fetch(e))
            })
        }
        oxc_ast::ast::Statement::IfStatement(if_stmt) => {
            if expr_has_fetch(&if_stmt.test) {
                return true;
            }
            if let oxc_ast::ast::Statement::BlockStatement(block) = &if_stmt.consequent
                && contains_top_level_fetch_stmts(&block.body) {
                    return true;
                }
            if_stmt.alternate.as_ref().is_some_and(|s| stmt_has_fetch(s))
        }
        oxc_ast::ast::Statement::BlockStatement(block) => {
            contains_top_level_fetch_stmts(&block.body)
        }
        oxc_ast::ast::Statement::ReturnStatement(ret) => {
            ret.argument.as_ref().is_some_and(|e| expr_has_fetch(e))
        }
        oxc_ast::ast::Statement::TryStatement(try_stmt) => {
            contains_top_level_fetch_stmts(&try_stmt.block.body)
                || try_stmt
                    .handler
                    .as_ref()
                    .is_some_and(|h| contains_top_level_fetch_stmts(&h.body.body))
        }
        _ => false,
    }
}

fn expr_has_fetch(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            if let Expression::Identifier(callee) = &call.callee
                && callee.name == "fetch" {
                    return true;
                }
            // Check arguments but don't cross function boundaries.
            call.arguments.iter().any(|arg| {
                let Some(e) = arg.as_expression() else { return false };
                match e {
                    Expression::ArrowFunctionExpression(_)
                    | Expression::FunctionExpression(_) => false,
                    _ => expr_has_fetch(e),
                }
            }) || match &call.callee {
                Expression::StaticMemberExpression(mem) => expr_has_fetch(&mem.object),
                _ => false,
            }
        }
        Expression::AwaitExpression(aw) => expr_has_fetch(&aw.argument),
        Expression::StaticMemberExpression(mem) => expr_has_fetch(&mem.object),
        Expression::ChainExpression(chain) => match &chain.expression {
            oxc_ast::ast::ChainElement::CallExpression(call) => {
                if let Expression::Identifier(callee) = &call.callee
                    && callee.name == "fetch" {
                        return true;
                    }
                false
            }
            _ => false,
        },
        // Don't cross function boundaries.
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => false,
        Expression::ConditionalExpression(cond) => {
            expr_has_fetch(&cond.test)
                || expr_has_fetch(&cond.consequent)
                || expr_has_fetch(&cond.alternate)
        }
        Expression::LogicalExpression(log) => {
            expr_has_fetch(&log.left) || expr_has_fetch(&log.right)
        }
        Expression::SequenceExpression(seq) => {
            seq.expressions.iter().any(|e| expr_has_fetch(e))
        }
        Expression::AssignmentExpression(assign) => expr_has_fetch(&assign.right),
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fetch"])
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
        if !is_effect_callee(&call.callee) {
            return;
        }
        if call.arguments.is_empty() {
            return;
        }

        let Some(callback_expr) = call.arguments[0].as_expression() else { return };
        let body_stmts = match callback_expr {
            Expression::ArrowFunctionExpression(arrow) => &arrow.body.statements,
            Expression::FunctionExpression(func) => {
                let Some(body) = &func.body else { return };
                &body.statements
            }
            _ => return,
        };

        if !contains_top_level_fetch_stmts(body_stmts) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`fetch()` in `useEffect` — use a data-fetching library (react-query, SWR) or a server component.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    // Regression for #911: a spread argument to useEffect made `Argument::to_expression()` panic.
    #[test]
    fn does_not_panic_on_spread_arg_to_use_effect() {
        assert!(run("useEffect(...args)").is_empty());
    }

    // Regression for #911: a spread argument inside a non-fetch call made `arg.to_expression()` panic.
    #[test]
    fn does_not_panic_on_spread_arg_inside_call() {
        assert!(run("useEffect(() => { f(...args); }, [])").is_empty());
    }
}
