//! generator-without-yield oxc backend — flag generator functions missing `yield`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Walk semantic descendants of a node to check if any is a YieldExpression,
/// but stop at nested function boundaries (they have their own generator scope).
fn has_yield_in_body<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let node_id = node.id();
    for snode in semantic.nodes().iter() {
        if let AstKind::YieldExpression(_) = snode.kind() {
            // Check if this yield's nearest function ancestor is our node.
            let mut cur = snode.id();
            loop {
                let parent_id = semantic.nodes().parent_id(cur);
                if parent_id == cur {
                    break;
                }
                if parent_id == node_id {
                    return true;
                }
                let parent = semantic.nodes().get_node(parent_id);
                // Stop at nested function boundaries.
                match parent.kind() {
                    AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => break,
                    _ => {}
                }
                cur = parent_id;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Function(func) = node.kind() else {
            return;
        };
        if !func.generator {
            return;
        }
        if has_yield_in_body(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, func.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Generator function does not contain a `yield` — add one or use a regular function."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
