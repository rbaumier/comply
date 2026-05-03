use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::ArrowFunctionExpression,
        ]
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
                let Some(body) = &func.body else { return };
                let mut return_types = HashSet::new();
                collect_return_types_from_stmts(&body.statements, &mut return_types);
                if let Some(diag) = check_return_types(&return_types, ctx, func.span.start as usize) {
                    diagnostics.push(diag);
                }
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                // Skip arrow functions with expression body (single return type).
                if arrow.expression {
                    return;
                }
                let mut return_types = HashSet::new();
                collect_return_types_from_stmts(&arrow.body.statements, &mut return_types);
                if let Some(diag) = check_return_types(&return_types, ctx, arrow.span.start as usize) {
                    diagnostics.push(diag);
                }
            }
            _ => {}
        }
    }
}

fn check_return_types(
    return_types: &HashSet<&str>,
    ctx: &CheckCtx,
    span_start: usize,
) -> Option<Diagnostic> {
    if return_types.len() < 2 {
        return None;
    }
    let has_null_or_undefined =
        return_types.contains("null") || return_types.contains("undefined");
    let non_nullish: Vec<_> = return_types
        .iter()
        .filter(|&&t| t != "null" && t != "undefined")
        .collect();
    if has_null_or_undefined && non_nullish.len() <= 1 {
        return None;
    }
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
    Some(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!("Function returns inconsistent types: {:?}", return_types),
        severity: Severity::Warning,
        span: None,
    })
}

fn collect_return_types_from_stmts<'a>(
    stmts: &'a [Statement<'a>],
    types: &mut HashSet<&'static str>,
) {
    for stmt in stmts {
        collect_return_types_from_stmt(stmt, types);
    }
}

fn collect_return_types_from_stmt<'a>(
    stmt: &'a Statement<'a>,
    types: &mut HashSet<&'static str>,
) {
    match stmt {
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                types.insert(infer_type(arg));
            }
        }
        // Don't descend into nested functions.
        Statement::FunctionDeclaration(_) => {}
        Statement::ExpressionStatement(expr) => {
            // Check for arrow/function expressions but don't descend.
            match &expr.expression {
                Expression::ArrowFunctionExpression(_)
                | Expression::FunctionExpression(_) => {}
                _ => collect_return_types_from_expr(&expr.expression, types),
            }
        }
        Statement::BlockStatement(block) => {
            collect_return_types_from_stmts(&block.body, types);
        }
        Statement::IfStatement(if_stmt) => {
            collect_return_types_from_stmt(&if_stmt.consequent, types);
            if let Some(alt) = &if_stmt.alternate {
                collect_return_types_from_stmt(alt, types);
            }
        }
        Statement::SwitchStatement(switch) => {
            for case in &switch.cases {
                collect_return_types_from_stmts(&case.consequent, types);
            }
        }
        Statement::TryStatement(try_stmt) => {
            collect_return_types_from_stmts(&try_stmt.block.body, types);
            if let Some(handler) = &try_stmt.handler {
                collect_return_types_from_stmts(&handler.body.body, types);
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                collect_return_types_from_stmts(&finalizer.body, types);
            }
        }
        Statement::ForStatement(f) => {
            collect_return_types_from_stmt(&f.body, types);
        }
        Statement::WhileStatement(w) => {
            collect_return_types_from_stmt(&w.body, types);
        }
        Statement::ForInStatement(f) => {
            collect_return_types_from_stmt(&f.body, types);
        }
        Statement::ForOfStatement(f) => {
            collect_return_types_from_stmt(&f.body, types);
        }
        Statement::DoWhileStatement(d) => {
            collect_return_types_from_stmt(&d.body, types);
        }
        Statement::LabeledStatement(l) => {
            collect_return_types_from_stmt(&l.body, types);
        }
        _ => {}
    }
}

fn collect_return_types_from_expr<'a>(
    _expr: &'a Expression<'a>,
    _types: &mut HashSet<&'static str>,
) {
    // Expression statements don't contain return statements at the top level.
}

fn infer_type(expr: &Expression) -> &'static str {
    match expr {
        Expression::NumericLiteral(_) => "number",
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => "string",
        Expression::BooleanLiteral(_) => "boolean",
        Expression::NullLiteral(_) => "null",
        Expression::ArrayExpression(_) => "array",
        Expression::ObjectExpression(_) => "object",
        Expression::Identifier(id) if id.name == "undefined" => "undefined",
        _ => "unknown",
    }
}
