//! OxcCheck backend for tanstack-start-server-fn-use-notfound.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["createServerFn"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(throw) = node.kind() else { return };
        let Expression::NewExpression(new_expr) = &throw.argument else { return };

        // Check constructor is `Error`
        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name != "Error" { return; }

        // Check argument contains "not found" (case-insensitive)
        let has_notfound_msg = new_expr.arguments.iter().any(|arg| {
            let Some(expr) = arg.as_expression() else { return false };
            match expr {
                Expression::StringLiteral(s) => s.value.to_ascii_lowercase().contains("not found"),
                Expression::TemplateLiteral(t) => {
                    t.quasis.iter().any(|q| q.value.raw.to_ascii_lowercase().contains("not found"))
                }
                _ => false,
            }
        });
        if !has_notfound_msg { return; }

        // Check if inside a createServerFn callback by walking ancestors.
        if !is_inside_create_server_fn(node, semantic) { return; }

        let (line, column) = byte_offset_to_line_col(ctx.source, throw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Throw `notFound()` instead of `new Error('...not found...')` so the \
                      router can render the 404 boundary."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_inside_create_server_fn(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor_id in semantic.nodes().ancestor_ids(node.id()) {
        let ancestor = semantic.nodes().get_node(ancestor_id);
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                if callback_belongs_to_create_server_fn(ancestor_id, semantic) {
                    return true;
                }
            }
            AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

fn callback_belongs_to_create_server_fn(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor_id in semantic.nodes().ancestor_ids(func_id) {
        let ancestor = semantic.nodes().get_node(ancestor_id);
        match ancestor.kind() {
            AstKind::CallExpression(call) => {
                if callee_chain_has_create_server_fn(&call.callee) {
                    return true;
                }
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return false;
            }
            _ => {}
        }
    }
    false
}

fn callee_chain_has_create_server_fn(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => id.name == "createServerFn",
        Expression::StaticMemberExpression(member) => {
            callee_chain_has_create_server_fn(&member.object)
        }
        Expression::CallExpression(call) => callee_chain_has_create_server_fn(&call.callee),
        _ => false,
    }
}
