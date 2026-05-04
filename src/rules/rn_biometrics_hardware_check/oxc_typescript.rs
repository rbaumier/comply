//! rn-biometrics-hardware-check OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Extract the callee name text from a CallExpression.
fn callee_name<'a>(call: &'a oxc_ast::ast::CallExpression<'a>, source: &'a str) -> &'a str {
    &source[call.callee.span().start as usize..call.callee.span().end as usize]
}

/// Walk all nodes inside a function body to find the earliest call ending with `needle`.
fn first_call_offset_in_function<'a>(
    func_node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &'a str,
    needle: &str,
) -> Option<u32> {
    let mut best: Option<u32> = None;
    // Walk all descendants of the function node
    for child in semantic.nodes().iter() {
        // Check if this node is a descendant of our function
        if !is_descendant(child.id(), func_node_id, semantic) {
            continue;
        }
        if let AstKind::CallExpression(call) = child.kind() {
            let name = callee_name(call, source);
            if name.ends_with(needle) {
                let start = call.span.start;
                best = Some(best.map_or(start, |b: u32| b.min(start)));
            }
        }
    }
    best
}

fn is_descendant(
    node_id: oxc_semantic::NodeId,
    ancestor_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut current = node_id;
    loop {
        if current == ancestor_id {
            return true;
        }
        let parent = semantic.nodes().parent_node(current);
        let parent_id = parent.id();
        if parent_id == current {
            return false;
        }
        current = parent_id;
    }
}

fn enclosing_function_node_id(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::NodeId> {
    let mut current = semantic.nodes().parent_node(node.id());
    loop {
        match current.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return Some(current.id());
            }
            AstKind::Program(_) => return None,
            _ => {}
        }
        let parent = semantic.nodes().parent_node(current.id());
        if parent.id() == current.id() {
            return None;
        }
        current = parent;
    }
}

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

        let name = callee_name(call, ctx.source);
        if !name.ends_with("authenticateAsync") {
            return;
        }

        let Some(func_id) = enclosing_function_node_id(node, semantic) else {
            return;
        };

        let auth_offset = call.span.start;
        let hw_before = first_call_offset_in_function(func_id, semantic, ctx.source, "hasHardwareAsync")
            .is_some_and(|o| o < auth_offset);
        let enrolled_before =
            first_call_offset_in_function(func_id, semantic, ctx.source, "isEnrolledAsync")
                .is_some_and(|o| o < auth_offset);

        if hw_before && enrolled_before {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`authenticateAsync` without `hasHardwareAsync` / `isEnrolledAsync` (in that order) \u{2014} the call can fail on devices without biometrics.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
