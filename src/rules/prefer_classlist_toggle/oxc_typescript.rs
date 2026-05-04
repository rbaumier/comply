//! OxcCheck backend for prefer-classlist-toggle.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ConditionalExpression,
            AstType::IfStatement,
            AstType::CallExpression,
        ]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["classList"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // Pattern 1: ternary — `cond ? el.classList.add('x') : el.classList.remove('x')`
            AstKind::ConditionalExpression(ternary) => {
                let cm = classlist_method_from_expr(&ternary.consequent);
                let am = classlist_method_from_expr(&ternary.alternate);
                if let (Some(m1), Some(m2)) = (cm, am) {
                    if (m1 == "add" && m2 == "remove") || (m1 == "remove" && m2 == "add") {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, ternary.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "prefer-classlist-toggle".into(),
                            message: "Prefer `classList.toggle('class', condition)` over conditional `classList.add/remove`.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
            // Pattern 2: if/else
            AstKind::IfStatement(if_stmt) => {
                let Some(alt) = &if_stmt.alternate else {
                    return;
                };
                let cons_method = find_classlist_call_in_stmt(&if_stmt.consequent);
                let alt_method = find_classlist_call_in_stmt(alt);
                if let (Some(m1), Some(m2)) = (cons_method, alt_method) {
                    if (m1 == "add" && m2 == "remove") || (m1 == "remove" && m2 == "add") {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, if_stmt.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "prefer-classlist-toggle".into(),
                            message: "Prefer `classList.toggle('class', condition)` over conditional `classList.add/remove`.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
            // Pattern 3: computed access — `el.classList[cond ? 'add' : 'remove']('x')`
            AstKind::CallExpression(call) => {
                let Expression::ComputedMemberExpression(computed) = &call.callee else {
                    return;
                };
                // Object must be `*.classList`
                let Expression::StaticMemberExpression(member) = &computed.object else {
                    return;
                };
                if member.property.name.as_str() != "classList" {
                    return;
                }
                // Check subscript for ternary with 'add'/'remove'
                let idx_src = &ctx.source[computed.expression.span().start as usize
                    ..computed.expression.span().end as usize];
                let has_add = idx_src.contains("'add'") || idx_src.contains("\"add\"");
                let has_remove = idx_src.contains("'remove'") || idx_src.contains("\"remove\"");
                if has_add && has_remove {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "prefer-classlist-toggle".into(),
                        message: "Prefer `classList.toggle('class', condition)` over conditional `classList.add/remove`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}

/// Check if an expression is a `*.classList.add(...)` or `*.classList.remove(...)` call.
fn classlist_method_from_expr<'a>(expr: &'a Expression<'a>) -> Option<&'static str> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let prop = member.property.name.as_str();
    if prop != "add" && prop != "remove" {
        return None;
    }
    // Object must be `*.classList`
    let Expression::StaticMemberExpression(obj_member) = &member.object else {
        return None;
    };
    if obj_member.property.name.as_str() != "classList" {
        return None;
    }
    if prop == "add" {
        Some("add")
    } else {
        Some("remove")
    }
}

fn find_classlist_call_in_stmt<'a>(stmt: &'a oxc_ast::ast::Statement<'a>) -> Option<&'static str> {
    match stmt {
        oxc_ast::ast::Statement::ExpressionStatement(es) => classlist_method_from_expr(&es.expression),
        oxc_ast::ast::Statement::BlockStatement(block) => {
            for s in &block.body {
                if let Some(m) = find_classlist_call_in_stmt(s) {
                    return Some(m);
                }
            }
            None
        }
        _ => None,
    }
}
