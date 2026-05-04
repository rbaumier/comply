//! zod-no-schema-in-hot-path OxcCheck backend — flag `z.*` calls whose nearest
//! enclosing function looks like a React component or a request handler, or
//! that sit inside a loop body.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // The callee must be a member expression chain rooted at `z`.
        if !is_z_chain(&call.callee, ctx.source) {
            return;
        }

        let nodes = semantic.nodes();
        let mut current_id = node.id();

        loop {
            let parent_id = nodes.parent_id(current_id);
            if parent_id == current_id {
                // Reached root.
                return;
            }
            let parent = nodes.get_node(parent_id);
            match parent.kind() {
                AstKind::ForStatement(_)
                | AstKind::ForInStatement(_)
                | AstKind::WhileStatement(_)
                | AstKind::DoWhileStatement(_) => {
                    let span = call.span();
                    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Zod schema built inside a loop body — hoist it outside the \
                                  loop so it is only constructed once."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
                AstKind::Function(func) => {
                    if is_hot_scope_oxc(func, parent_id, semantic, ctx.source) {
                        let span = call.span();
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Zod schema built inside a React component or request \
                                      handler — hoist it to module scope so it is only \
                                      constructed once."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    return;
                }
                AstKind::ArrowFunctionExpression(_) => {
                    if is_hot_scope_arrow(parent_id, semantic, ctx.source) {
                        let span = call.span();
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Zod schema built inside a React component or request \
                                      handler — hoist it to module scope so it is only \
                                      constructed once."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    return;
                }
                AstKind::Program(_) => return,
                _ => {}
            }
            current_id = parent_id;
        }
    }
}

fn starts_uppercase(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
}

/// Check if a `z.*` member expression chain is rooted at the identifier `z`.
fn is_z_chain(expr: &Expression<'_>, source: &str) -> bool {
    match expr {
        Expression::StaticMemberExpression(member) => is_z_chain(&member.object, source),
        Expression::ComputedMemberExpression(member) => is_z_chain(&member.object, source),
        Expression::CallExpression(call) => is_z_chain(&call.callee, source),
        Expression::Identifier(ident) => ident.name == "z",
        _ => false,
    }
}

fn function_name_from_oxc<'a>(
    func: &oxc_ast::ast::Function<'a>,
    func_node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> Option<String> {
    // function_declaration: has its own name.
    if let Some(id) = &func.id {
        return Some(id.name.to_string());
    }
    // Arrow assigned to variable_declarator: check parent.
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(func_node_id);
    if parent_id != func_node_id
        && let AstKind::VariableDeclarator(decl) = nodes.get_node(parent_id).kind() {
            let name = &source[decl.id.span().start as usize..decl.id.span().end as usize];
            return Some(name.to_string());
        }
    None
}

fn looks_like_handler_params(params: &oxc_ast::ast::FormalParameters<'_>, source: &str) -> bool {
    let text = &source[params.span.start as usize..params.span.end as usize];
    text.contains("req")
        || text.contains("request")
        || text.contains("ctx")
        || text.contains("res")
        || text.contains("response")
}

fn is_hot_scope_oxc<'a>(
    func: &oxc_ast::ast::Function<'a>,
    func_node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    if let Some(name) = function_name_from_oxc(func, func_node_id, semantic, source)
        && starts_uppercase(&name) && name != "Check" {
            return true;
        }
    looks_like_handler_params(&func.params, source)
}

fn is_hot_scope_arrow<'a>(
    arrow_node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let arrow = nodes.get_node(arrow_node_id);
    let AstKind::ArrowFunctionExpression(arrow_expr) = arrow.kind() else {
        return false;
    };

    // Check parent for variable name (arrow assigned to const).
    let parent_id = nodes.parent_id(arrow_node_id);
    if parent_id != arrow_node_id
        && let AstKind::VariableDeclarator(decl) = nodes.get_node(parent_id).kind() {
            let name = &source[decl.id.span().start as usize..decl.id.span().end as usize];
            if starts_uppercase(name) && name != "Check" {
                return true;
            }
        }

    looks_like_handler_params(&arrow_expr.params, source)
}
