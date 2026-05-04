//! no-redundant-boolean OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn push_diag(
    diagnostics: &mut Vec<Diagnostic>,
    ctx: &CheckCtx,
    span: oxc_span::Span,
    message: &str,
) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: message.into(),
        severity: Severity::Error,
        span: None,
    });
}

fn is_bool_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::BooleanLiteral(_))
}

fn bool_value(expr: &Expression) -> Option<bool> {
    if let Expression::BooleanLiteral(lit) = expr {
        Some(lit.value)
    } else {
        None
    }
}

/// If a statement is a return statement returning a boolean literal,
/// return that boolean's value.
fn returns_bool(stmt: &Statement) -> Option<bool> {
    match stmt {
        Statement::ReturnStatement(ret) => {
            ret.argument.as_ref().and_then(|arg| bool_value(arg))
        }
        Statement::BlockStatement(block) => {
            if block.body.len() == 1 {
                returns_bool(&block.body[0])
            } else {
                None
            }
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ConditionalExpression,
            AstType::BinaryExpression,
            AstType::IfStatement,
        ]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["true", "false"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // Pattern 1: ternary with boolean literal branches.
            AstKind::ConditionalExpression(ternary) => {
                if is_bool_literal(&ternary.consequent) && is_bool_literal(&ternary.alternate) {
                    push_diag(
                        diagnostics,
                        ctx,
                        ternary.span,
                        "Redundant ternary — simplify to the condition itself (or its negation).",
                    );
                }
            }

            // Pattern 2: strict comparison against a boolean literal.
            AstKind::BinaryExpression(bin) => {
                if bin.operator != BinaryOperator::StrictEquality
                    && bin.operator != BinaryOperator::StrictInequality
                {
                    return;
                }
                if is_bool_literal(&bin.left) || is_bool_literal(&bin.right) {
                    push_diag(
                        diagnostics,
                        ctx,
                        bin.span,
                        "Redundant boolean comparison — use the value directly.",
                    );
                }
            }

            // Pattern 3: if/else returning boolean literals.
            AstKind::IfStatement(if_stmt) => {
                let Some(cons_bool) = returns_bool(&if_stmt.consequent) else {
                    return;
                };

                // 3a. Explicit else branch.
                if let Some(ref alt) = if_stmt.alternate {
                    if let Some(alt_bool) = returns_bool(alt) {
                        if cons_bool != alt_bool {
                            push_diag(
                                diagnostics,
                                ctx,
                                if_stmt.span,
                                "Redundant if/else returning boolean literals — return the condition directly.",
                            );
                        }
                    }
                    return;
                }

                // 3b. No else — look at the next sibling statement.
                // Walk the parent to find the sibling after this if.
                let nodes = semantic.nodes();
                let parent_id = nodes.parent_id(node.id());
                if parent_id == node.id() {
                    return;
                }
                let parent_kind = nodes.kind(parent_id);
                let stmts: Option<&oxc_allocator::Vec<Statement>> = match parent_kind {
                    AstKind::FunctionBody(body) => Some(&body.statements),
                    AstKind::BlockStatement(block) => Some(&block.body),
                    _ => None,
                };
                if let Some(stmts) = stmts {
                    let mut found_self = false;
                    for stmt in stmts.iter() {
                        if found_self {
                            if let Some(next_bool) = returns_bool(stmt) {
                                if cons_bool != next_bool {
                                    push_diag(
                                        diagnostics,
                                        ctx,
                                        if_stmt.span,
                                        "Redundant if/else returning boolean literals — return the condition directly.",
                                    );
                                }
                            }
                            break;
                        }
                        if let Statement::IfStatement(s) = stmt {
                            if s.span == if_stmt.span {
                                found_self = true;
                            }
                        }
                    }
                }
            }

            _ => {}
        }
    }
}
