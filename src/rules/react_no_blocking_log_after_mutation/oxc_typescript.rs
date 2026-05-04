//! OxcCheck backend for react-no-blocking-log-after-mutation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

const LOG_NAMES: &[&str] = &[
    "log",
    "logger",
    "analytics",
    "track",
    "telemetry",
    "metrics",
];

fn is_log_target(name: &str) -> bool {
    LOG_NAMES.contains(&name)
}

fn await_call_target<'a>(await_expr: &'a oxc_ast::ast::AwaitExpression<'a>) -> Option<&'a str> {
    let Expression::CallExpression(call) = &await_expr.argument else {
        return None;
    };
    match &call.callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                Some(obj.name.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Collect await expressions from statements, not descending into nested functions.
fn collect_awaits_from_stmts<'a>(
    stmts: &'a [Statement<'a>],
    out: &mut Vec<(oxc_span::Span, bool)>,
) {
    for stmt in stmts {
        collect_awaits_from_stmt(stmt, out);
    }
}

fn collect_awaits_from_stmt<'a>(stmt: &'a Statement<'a>, out: &mut Vec<(oxc_span::Span, bool)>) {
    match stmt {
        Statement::ExpressionStatement(es) => collect_awaits_from_expr(&es.expression, out),
        Statement::VariableDeclaration(decl) => {
            for d in &decl.declarations {
                if let Some(init) = &d.init {
                    collect_awaits_from_expr(init, out);
                }
            }
        }
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                collect_awaits_from_expr(arg, out);
            }
        }
        Statement::IfStatement(ifs) => {
            collect_awaits_from_stmt(&ifs.consequent, out);
            if let Some(alt) = &ifs.alternate {
                collect_awaits_from_stmt(alt, out);
            }
        }
        Statement::BlockStatement(block) => {
            collect_awaits_from_stmts(&block.body, out);
        }
        Statement::TryStatement(ts) => {
            collect_awaits_from_stmts(&ts.block.body, out);
            if let Some(handler) = &ts.handler {
                collect_awaits_from_stmts(&handler.body.body, out);
            }
            if let Some(finalizer) = &ts.finalizer {
                collect_awaits_from_stmts(&finalizer.body, out);
            }
        }
        // Don't descend into nested function/arrow/class declarations
        _ => {}
    }
}

fn collect_awaits_from_expr<'a>(expr: &'a Expression<'a>, out: &mut Vec<(oxc_span::Span, bool)>) {
    match expr {
        Expression::AwaitExpression(await_expr) => {
            let is_log = await_call_target(await_expr)
                .map(is_log_target)
                .unwrap_or(false);
            out.push((await_expr.span, is_log));
        }
        Expression::SequenceExpression(seq) => {
            for e in &seq.expressions {
                collect_awaits_from_expr(e, out);
            }
        }
        Expression::ConditionalExpression(cond) => {
            collect_awaits_from_expr(&cond.test, out);
            collect_awaits_from_expr(&cond.consequent, out);
            collect_awaits_from_expr(&cond.alternate, out);
        }
        Expression::AssignmentExpression(assign) => {
            collect_awaits_from_expr(&assign.right, out);
        }
        // Don't descend into nested functions
        _ => {}
    }
}

fn check_body(body: &oxc_ast::ast::FunctionBody, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let mut awaits = Vec::new();
    collect_awaits_from_stmts(&body.statements, &mut awaits);

    let mut saw_non_log = false;
    for (span, is_log) in awaits {
        if !is_log {
            saw_non_log = true;
            continue;
        }
        if saw_non_log {
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`await` on a log/analytics/track call after a main mutation blocks the response — \
                          drop the `await` or use `after()`/`waitUntil()`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportNamedDeclaration, AstType::ExportDefaultDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ExportNamedDeclaration(export) => {
                let Some(decl) = &export.declaration else { return };
                match decl {
                    oxc_ast::ast::Declaration::FunctionDeclaration(func) => {
                        if !func.r#async {
                            return;
                        }
                        if let Some(body) = &func.body {
                            check_body(body, ctx, diagnostics);
                        }
                    }
                    oxc_ast::ast::Declaration::VariableDeclaration(var_decl) => {
                        for declarator in &var_decl.declarations {
                            let Some(init) = &declarator.init else { continue };
                            match init {
                                Expression::ArrowFunctionExpression(arrow) => {
                                    if !arrow.r#async {
                                        continue;
                                    }
                                    check_body(&arrow.body, ctx, diagnostics);
                                }
                                Expression::FunctionExpression(func) => {
                                    if !func.r#async {
                                        continue;
                                    }
                                    if let Some(body) = &func.body {
                                        check_body(body, ctx, diagnostics);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            AstKind::ExportDefaultDeclaration(export) => {
                if let oxc_ast::ast::ExportDefaultDeclarationKind::FunctionDeclaration(func) = &export.declaration {
                    if !func.r#async {
                        return;
                    }
                    if let Some(body) = &func.body {
                        check_body(body, ctx, diagnostics);
                    }
                }
            }
            _ => {}
        }
    }
}
