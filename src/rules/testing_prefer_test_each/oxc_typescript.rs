use std::sync::Arc;

use oxc_ast::AstKind;
use oxc_ast::ast::{Expression, Statement};
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Returns `true` if `expr` is `test(...)` / `it(...)` / `test.only(...)`
/// / `it.skip(...)` etc.
fn is_test_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    match &call.callee {
        Expression::Identifier(ident) => matches!(ident.name.as_str(), "test" | "it"),
        Expression::StaticMemberExpression(member) => {
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            matches!(obj.name.as_str(), "test" | "it")
        }
        _ => false,
    }
}

/// Recursively check if any statement contains a test call.
fn stmts_contain_test_call(stmts: &[Statement]) -> bool {
    for stmt in stmts {
        match stmt {
            Statement::ExpressionStatement(es) => {
                if is_test_call(&es.expression) {
                    return true;
                }
            }
            Statement::BlockStatement(block) => {
                if stmts_contain_test_call(&block.body) {
                    return true;
                }
            }
            Statement::IfStatement(if_stmt) => {
                if stmt_contains_test_call(&if_stmt.consequent) {
                    return true;
                }
                if let Some(alt) = &if_stmt.alternate {
                    if stmt_contains_test_call(alt) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn stmt_contains_test_call(stmt: &Statement) -> bool {
    stmts_contain_test_call(std::slice::from_ref(stmt))
}

fn body_contains_test_call(body: &Statement) -> bool {
    match body {
        Statement::BlockStatement(block) => stmts_contain_test_call(&block.body),
        other => stmt_contains_test_call(other),
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ForStatement,
            AstType::ForInStatement,
            AstType::ForOfStatement,
            AstType::WhileStatement,
            AstType::CallExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }

        match node.kind() {
            AstKind::ForStatement(stmt) => {
                if body_contains_test_call(&stmt.body) {
                    push(diagnostics, ctx, stmt.span.start, "for");
                }
            }
            AstKind::ForInStatement(stmt) => {
                if body_contains_test_call(&stmt.body) {
                    push(diagnostics, ctx, stmt.span.start, "for_in");
                }
            }
            AstKind::ForOfStatement(stmt) => {
                if body_contains_test_call(&stmt.body) {
                    push(diagnostics, ctx, stmt.span.start, "for_in");
                }
            }
            AstKind::WhileStatement(stmt) => {
                if body_contains_test_call(&stmt.body) {
                    push(diagnostics, ctx, stmt.span.start, "while");
                }
            }
            AstKind::CallExpression(call) => {
                // Match `xs.forEach(cb)` where `cb` body has a test call.
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return;
                };
                if member.property.name.as_str() != "forEach" {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else {
                    return;
                };
                let Some(expr) = first_arg.as_expression() else {
                    return;
                };
                match expr {
                    Expression::ArrowFunctionExpression(arrow) => {
                        if stmts_contain_test_call(&arrow.body.statements) {
                            push(diagnostics, ctx, call.span.start, "forEach");
                        }
                    }
                    Expression::FunctionExpression(func) => {
                        if let Some(body) = &func.body {
                            if stmts_contain_test_call(&body.statements) {
                                push(diagnostics, ctx, call.span.start, "forEach");
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn push(diagnostics: &mut Vec<Diagnostic>, ctx: &CheckCtx, span_start: u32, kind: &str) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "`{kind}` wraps a `test` / `it` call — replace the loop with `test.each(cases)(...)` so each row is a separate named case."
        ),
        severity: Severity::Warning,
        span: None,
    });
}
