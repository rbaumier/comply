//! playwright-prefer-hooks-in-order OXC backend — enforce consistent hook ordering.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const HOOK_ORDER: &[&str] = &["beforeAll", "beforeEach", "afterEach", "afterAll"];

fn hook_index(name: &str) -> Option<usize> {
    HOOK_ORDER.iter().position(|&h| h == name)
}

/// Extract hook name from a call expression callee.
fn get_hook_name_from_callee<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
    match callee {
        Expression::Identifier(id) => {
            let name = id.name.as_str();
            if HOOK_ORDER.contains(&name) {
                Some(name)
            } else {
                None
            }
        }
        Expression::StaticMemberExpression(member) => {
            let name = member.property.name.as_str();
            if HOOK_ORDER.contains(&name) {
                Some(name)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Check if a callee is a `describe` call.
fn is_describe_callee(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => id.name.as_str() == "describe",
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "describe"
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Get the body of the last argument (callback) of a call expression.
fn get_callback_body<'a>(
    args: &'a oxc_allocator::Vec<'a, Argument<'a>>,
) -> Option<&'a oxc_ast::ast::FunctionBody<'a>> {
    let last = args.last()?;
    let expr = last.as_expression()?;
    match expr {
        Expression::ArrowFunctionExpression(arrow) => Some(&arrow.body),
        Expression::FunctionExpression(func) => func.body.as_deref(),
        _ => None,
    }
}

fn check_hook_order_in_body(
    body: &oxc_ast::ast::FunctionBody,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut prev_index: Option<usize> = None;

    for stmt in &body.statements {
        let Statement::ExpressionStatement(expr_stmt) = stmt else {
            continue;
        };
        let Expression::CallExpression(call) = &expr_stmt.expression else {
            continue;
        };

        // Recurse into describe callbacks.
        if is_describe_callee(&call.callee) {
            if let Some(cb_body) = get_callback_body(&call.arguments) {
                check_hook_order_in_body(cb_body, ctx, diagnostics);
            }
            continue;
        }

        if let Some(name) = get_hook_name_from_callee(&call.callee) {
            let idx = hook_index(name).unwrap();
            if let Some(prev) = prev_index {
                if idx < prev {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{name}` hooks should be before any `{}` hooks.",
                            HOOK_ORDER[prev]
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            prev_index = Some(idx);
        } else {
            prev_index = None;
        }
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        // Find the program node and check its body.
        for node in semantic.nodes().iter() {
            if let AstKind::Program(prog) = node.kind() {
                check_program_stmts(&prog.body, ctx, &mut diagnostics);
                break;
            }
        }

        diagnostics
    }
}

fn check_program_stmts(
    stmts: &oxc_allocator::Vec<'_, Statement<'_>>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut prev_index: Option<usize> = None;

    for stmt in stmts {
        let Statement::ExpressionStatement(expr_stmt) = stmt else {
            continue;
        };
        let Expression::CallExpression(call) = &expr_stmt.expression else {
            continue;
        };

        if is_describe_callee(&call.callee) {
            if let Some(cb_body) = get_callback_body(&call.arguments) {
                check_hook_order_in_body(cb_body, ctx, diagnostics);
            }
            continue;
        }

        if let Some(name) = get_hook_name_from_callee(&call.callee) {
            let idx = hook_index(name).unwrap();
            if let Some(prev) = prev_index {
                if idx < prev {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{name}` hooks should be before any `{}` hooks.",
                            HOOK_ORDER[prev]
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            prev_index = Some(idx);
        } else {
            prev_index = None;
        }
    }
}
