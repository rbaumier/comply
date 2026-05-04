//! OxcCheck backend for no-conditional-async-return.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReturnKind {
    Sync,
    Promise,
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::Function(func) => {
                    if func.r#async {
                        continue;
                    }
                    let Some(body) = &func.body else {
                        continue;
                    };
                    let kinds = collect_return_kinds(&body.statements, ctx.source);
                    if kinds.contains(&ReturnKind::Sync) && kinds.contains(&ReturnKind::Promise) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, func.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Function mixes sync and promise-returning branches — unify to `Promise<T>` (async) or plain `T` everywhere.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    if arrow.r#async {
                        continue;
                    }
                    if arrow.expression {
                        continue;
                    }
                    let kinds = collect_return_kinds(&arrow.body.statements, ctx.source);
                    if kinds.contains(&ReturnKind::Sync) && kinds.contains(&ReturnKind::Promise) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Function mixes sync and promise-returning branches — unify to `Promise<T>` (async) or plain `T` everywhere.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

/// Classify a return-value expression as promise-returning or sync.
fn classify_value(expr: &Expression, _source: &str) -> ReturnKind {
    let Expression::CallExpression(call) = expr else {
        return ReturnKind::Sync;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return ReturnKind::Sync;
    };
    let method = member.property.name.as_str();

    // `.then(...)` / `.catch(...)` / `.finally(...)` on any receiver
    if method == "then" || method == "catch" || method == "finally" {
        return ReturnKind::Promise;
    }

    // `Promise.<combinator>(...)`
    if let Expression::Identifier(obj) = &member.object
        && obj.name.as_str() == "Promise"
            && matches!(
                method,
                "resolve" | "reject" | "all" | "allSettled" | "race" | "any"
            )
        {
            return ReturnKind::Promise;
        }

    ReturnKind::Sync
}

/// Walk statements collecting return kinds. Skip nested function bodies.
fn collect_return_kinds(stmts: &[Statement], source: &str) -> Vec<ReturnKind> {
    let mut out = Vec::new();
    for stmt in stmts {
        collect_from_stmt(stmt, source, &mut out);
    }
    out
}

fn collect_from_stmt(stmt: &Statement, source: &str, out: &mut Vec<ReturnKind>) {
    match stmt {
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                out.push(classify_value(arg, source));
            }
        }
        // Don't descend into nested functions
        Statement::FunctionDeclaration(_) => {}
        Statement::BlockStatement(block) => {
            for s in &block.body {
                collect_from_stmt(s, source, out);
            }
        }
        Statement::IfStatement(if_stmt) => {
            collect_from_stmt(&if_stmt.consequent, source, out);
            if let Some(alt) = &if_stmt.alternate {
                collect_from_stmt(alt, source, out);
            }
        }
        Statement::SwitchStatement(switch) => {
            for case in &switch.cases {
                for s in &case.consequent {
                    collect_from_stmt(s, source, out);
                }
            }
        }
        Statement::TryStatement(try_stmt) => {
            for s in &try_stmt.block.body {
                collect_from_stmt(s, source, out);
            }
            if let Some(handler) = &try_stmt.handler {
                for s in &handler.body.body {
                    collect_from_stmt(s, source, out);
                }
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                for s in &finalizer.body {
                    collect_from_stmt(s, source, out);
                }
            }
        }
        Statement::ForStatement(for_stmt) => {
            collect_from_stmt(&for_stmt.body, source, out);
        }
        Statement::ForInStatement(for_in) => {
            collect_from_stmt(&for_in.body, source, out);
        }
        Statement::ForOfStatement(for_of) => {
            collect_from_stmt(&for_of.body, source, out);
        }
        Statement::WhileStatement(while_stmt) => {
            collect_from_stmt(&while_stmt.body, source, out);
        }
        Statement::DoWhileStatement(do_while) => {
            collect_from_stmt(&do_while.body, source, out);
        }
        Statement::LabeledStatement(labeled) => {
            collect_from_stmt(&labeled.body, source, out);
        }
        // ExpressionStatement containing arrow/function — skip (nested fn)
        Statement::ExpressionStatement(es) => {
            if matches!(
                es.expression,
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            ) {
                // nested function — don't descend
            }
        }
        _ => {}
    }
}
