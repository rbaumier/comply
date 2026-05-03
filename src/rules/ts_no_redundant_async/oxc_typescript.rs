//! OxcCheck backend for ts-no-redundant-async.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

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
                    if !func.r#async {
                        continue;
                    }
                    let Some(ref body) = func.body else { continue };
                    if !is_single_return_await_block(body) {
                        continue;
                    }
                    if body_has_try(body) {
                        continue;
                    }
                    if count_awaits_in_body(body.span, semantic) != 1 {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, func.span.start as usize);
                    diagnostics.push(make_diag(ctx, line, column));
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    if !arrow.r#async {
                        continue;
                    }
                    // Expression body: `async () => await x`
                    if arrow.expression {
                        let is_await = arrow.body.statements.first().is_some_and(|stmt| {
                            if let Statement::ExpressionStatement(es) = stmt {
                                matches!(es.expression, Expression::AwaitExpression(_))
                            } else {
                                false
                            }
                        });
                        if is_await && !body_has_try(&arrow.body) {
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
                            diagnostics.push(make_diag(ctx, line, column));
                        }
                        continue;
                    }
                    // Block body
                    if !is_single_return_await_block(&arrow.body) {
                        continue;
                    }
                    if body_has_try(&arrow.body) {
                        continue;
                    }
                    if count_awaits_in_body(arrow.body.span, semantic) != 1 {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
                    diagnostics.push(make_diag(ctx, line, column));
                }
                _ => {}
            }
        }

        diagnostics
    }
}

fn make_diag(ctx: &CheckCtx, line: usize, column: usize) -> Diagnostic {
    Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Redundant `async`/`await`: this function only does `return await expr` \
                  with no try/catch — drop `async` and `await` and return the promise directly."
            .into(),
        severity: Severity::Warning,
        span: None,
    }
}

/// Check a function body block: exactly one statement which is `return await X`.
fn is_single_return_await_block(body: &FunctionBody) -> bool {
    if body.statements.len() != 1 {
        return false;
    }
    let Statement::ReturnStatement(ret) = &body.statements[0] else {
        return false;
    };
    let Some(ref arg) = ret.argument else {
        return false;
    };
    matches!(arg, Expression::AwaitExpression(_))
}

fn body_has_try(body: &FunctionBody) -> bool {
    body.statements
        .iter()
        .any(|s| matches!(s, Statement::TryStatement(_)))
}

/// Count await expressions within the given span, skipping nested functions.
fn count_awaits_in_body(body_span: oxc_span::Span, semantic: &oxc_semantic::Semantic) -> usize {
    let mut count = 0;
    for node in semantic.nodes().iter() {
        if let AstKind::AwaitExpression(aw) = node.kind() {
            if aw.span.start >= body_span.start && aw.span.end <= body_span.end {
                // Check it's not inside a nested function.
                if !is_inside_nested_function(node, body_span, semantic) {
                    count += 1;
                }
            }
        }
    }
    count
}

fn is_inside_nested_function(
    node: &oxc_semantic::AstNode,
    outer_body_span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor_id in semantic.nodes().ancestor_ids(node.id()) {
        let ancestor = semantic.nodes().get_node(ancestor_id);
        let ancestor_span = match ancestor.kind() {
            AstKind::Function(f) => Some(f.span),
            AstKind::ArrowFunctionExpression(a) => Some(a.span),
            _ => None,
        };
        if let Some(ps) = ancestor_span {
            // If this function is strictly inside the outer body, it's nested.
            if ps.start > outer_body_span.start && ps.end <= outer_body_span.end {
                return true;
            }
            // If we've reached the outer function boundary, stop.
            if ps.start <= outer_body_span.start {
                return false;
            }
        }
    }
    false
}
