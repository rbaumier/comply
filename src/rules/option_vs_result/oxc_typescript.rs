//! option-vs-result OXC backend — find*/get* functions returning null/undefined.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, Statement};
use std::sync::Arc;

pub struct Check;

/// Returns true if the identifier starts with `find` or `get` followed by
/// an uppercase letter (camelCase convention).
fn is_find_or_get(name: &str) -> bool {
    for prefix in &["find", "get"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            if rest.starts_with(|c: char| c.is_ascii_uppercase()) {
                return true;
            }
        }
    }
    false
}

/// Recursively check if any statement in the body is `return null` or `return undefined`.
/// Doesn't descend into nested functions.
fn body_has_null_return(stmts: &oxc_allocator::Vec<'_, Statement<'_>>) -> bool {
    for stmt in stmts.iter() {
        if has_null_return_stmt(stmt) {
            return true;
        }
    }
    false
}

fn has_null_return_stmt(stmt: &Statement<'_>) -> bool {
    match stmt {
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                return is_null_or_undefined_expr(arg);
            }
            false
        }
        Statement::BlockStatement(block) => {
            for s in block.body.iter() {
                if has_null_return_stmt(s) {
                    return true;
                }
            }
            false
        }
        Statement::IfStatement(if_stmt) => {
            if has_null_return_stmt(&if_stmt.consequent) {
                return true;
            }
            if let Some(alt) = &if_stmt.alternate {
                return has_null_return_stmt(alt);
            }
            false
        }
        Statement::TryStatement(try_stmt) => {
            for s in try_stmt.block.body.iter() {
                if has_null_return_stmt(s) {
                    return true;
                }
            }
            if let Some(handler) = &try_stmt.handler {
                for s in handler.body.body.iter() {
                    if has_null_return_stmt(s) {
                        return true;
                    }
                }
            }
            false
        }
        Statement::SwitchStatement(sw) => {
            for case in &sw.cases {
                for s in &case.consequent {
                    if has_null_return_stmt(s) {
                        return true;
                    }
                }
            }
            false
        }
        // Don't descend into nested function declarations.
        Statement::FunctionDeclaration(_) => false,
        _ => false,
    }
}

fn is_null_or_undefined_expr(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::NullLiteral(_) => true,
        Expression::Identifier(id) => id.name.as_str() == "undefined",
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::VariableDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Function(func) => {
                let Some(id) = &func.id else { return };
                let name = id.name.as_str();
                if !is_find_or_get(name) {
                    return;
                }
                let Some(body) = &func.body else { return };
                if !body_has_null_return(&body.statements) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, func.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Function named `find*`/`get*` returns `null`/`undefined` — \
                              consider using an Option type to make absence explicit."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                        continue;
                    };
                    let name = id.name.as_str();
                    if !is_find_or_get(name) {
                        continue;
                    }
                    let Some(init) = &declarator.init else { continue };
                    let func_body = match init {
                        Expression::ArrowFunctionExpression(arrow) => Some(&arrow.body),
                        Expression::FunctionExpression(func) => {
                            func.body.as_ref()
                        }
                        _ => None,
                    };
                    let Some(body) = func_body else { continue };
                    if !body_has_null_return(&body.statements) {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, decl.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Function named `find*`/`get*` returns `null`/`undefined` — \
                                  consider using an Option type to make absence explicit."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}
