//! no-property-mutation OXC backend — flag property mutations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const TEST_GLOBALS: &[&str] = &["console", "window", "global", "globalThis", "process"];
const TEST_HOOKS: &[&str] = &["beforeEach", "afterEach", "beforeAll", "afterAll"];

/// Get the root object identifier name from an expression chain.
fn root_object_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => root_object_name(&m.object),
        Expression::ComputedMemberExpression(m) => root_object_name(&m.object),
        _ => None,
    }
}

fn is_inside_test_hook<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind() {
            let callee_name = match &call.callee {
                Expression::Identifier(id) => Some(id.name.as_str()),
                Expression::StaticMemberExpression(m) => Some(m.property.name.as_str()),
                _ => None,
            };
            if callee_name.is_some_and(|name| TEST_HOOKS.contains(&name)) {
                return true;
            }
        }
    }
    false
}

fn is_test_setup_for_expr<'a>(
    node: &oxc_semantic::AstNode<'a>,
    obj_expr: &Expression<'a>,
    ctx: &CheckCtx,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    if !ctx.file.path_segments.in_test_dir {
        return false;
    }
    if root_object_name(obj_expr).is_some_and(|name| TEST_GLOBALS.contains(&name)) {
        return true;
    }
    is_inside_test_hook(node, semantic)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::AssignmentExpression,
            AstType::UpdateExpression,
            AstType::UnaryExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::AssignmentExpression(assign) => {
                match &assign.left {
                    AssignmentTarget::StaticMemberExpression(m) => {
                        let obj_text = &ctx.source
                            [m.object.span().start as usize..m.object.span().end as usize];
                        let prop_text = m.property.name.as_str();

                        if obj_text == "module" || obj_text == "exports" { return; }
                        if prop_text == "current" { return; }
                        if obj_text == "document" && prop_text == "cookie" { return; }
                        if root_object_name(&m.object) == Some("set") { return; }
                        if is_test_setup_for_expr(node, &m.object, ctx, semantic) { return; }

                        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation — use spread or immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    AssignmentTarget::ComputedMemberExpression(m) => {
                        let obj_text = &ctx.source
                            [m.object.span().start as usize..m.object.span().end as usize];

                        if obj_text == "module" || obj_text == "exports" { return; }
                        if root_object_name(&m.object) == Some("set") { return; }
                        if is_test_setup_for_expr(node, &m.object, ctx, semantic) { return; }

                        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation — use spread or immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    _ => {}
                }
            }
            AstKind::UpdateExpression(update) => {
                // update.argument is a SimpleAssignmentTarget.
                // Check if it's a member expression.
                match &update.argument {
                    SimpleAssignmentTarget::StaticMemberExpression(m) => {
                        if is_test_setup_for_expr(node, &m.object, ctx, semantic) { return; }
                        let (line, column) = byte_offset_to_line_col(ctx.source, update.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation (increment/decrement) — use immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    SimpleAssignmentTarget::ComputedMemberExpression(m) => {
                        if is_test_setup_for_expr(node, &m.object, ctx, semantic) { return; }
                        let (line, column) = byte_offset_to_line_col(ctx.source, update.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation (increment/decrement) — use immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    _ => {}
                }
            }
            AstKind::UnaryExpression(unary) => {
                if unary.operator != UnaryOperator::Delete {
                    return;
                }
                match &unary.argument {
                    Expression::StaticMemberExpression(m) => {
                        if is_test_setup_for_expr(node, &m.object, ctx, semantic) { return; }
                    }
                    Expression::ComputedMemberExpression(m) => {
                        if is_test_setup_for_expr(node, &m.object, ctx, semantic) { return; }
                    }
                    _ => return,
                }

                let (line, column) = byte_offset_to_line_col(ctx.source, unary.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-property-mutation".into(),
                    message: "Property deletion — use destructuring or immutable patterns.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
