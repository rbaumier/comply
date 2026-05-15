//! promise-catch-or-return oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Walk up the chain from the outer `.then(...)` call. Returns true if
/// any chained method is `.catch` / `.finally` (which handles rejection)
/// OR the chain is returned / awaited / yielded.
fn chain_is_safe<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut current_id = node.id();
    let nodes = semantic.nodes();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::StaticMemberExpression(_) => {
                current_id = parent_id;
                continue;
            }
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(member) = &call.callee
                    && matches!(member.property.name.as_str(), "catch" | "finally")
                {
                    return true;
                }
                current_id = parent_id;
                continue;
            }
            AstKind::ReturnStatement(_)
            | AstKind::AwaitExpression(_)
            | AstKind::YieldExpression(_) => return true,
            AstKind::ArrowFunctionExpression(a) if a.expression => return true,
            AstKind::VariableDeclarator(_) | AstKind::AssignmentExpression(_) => {
                return true
            }
            AstKind::ExpressionStatement(_) => return false,
            _ => {
                current_id = parent_id;
            }
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".then("])
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
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "then" {
            return;
        }
        // Already chained from another .then — we're not the outermost.
        // Walk up: if any parent is itself a `.then(...)` call chain
        // we don't want to flag again here.
        let parent_id = semantic.nodes().parent_id(node.id());
        if let AstKind::StaticMemberExpression(_) = semantic.nodes().get_node(parent_id).kind() {
            return;
        }
        if chain_is_safe(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Floating `.then(...)` without a `.catch` / `.finally` and not \
                      returned/awaited — rejection will be swallowed."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
