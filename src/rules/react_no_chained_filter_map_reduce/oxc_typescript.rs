//! react-no-chained-filter-map-reduce OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const CHAIN_METHODS: &[&str] = &["filter", "map", "reduce", "flatMap"];

fn method_name_of_call<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Option<&'a str> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let name = member.property.name.as_str();
    if CHAIN_METHODS.contains(&name) {
        Some(name)
    } else {
        None
    }
}

fn chain_length<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> u32 {
    let mut count = 0u32;
    let mut current = call;
    loop {
        if method_name_of_call(current).is_none() {
            return count;
        }
        count += 1;
        // Get the receiver (the object of the member expression)
        let Expression::StaticMemberExpression(member) = &current.callee else {
            return count;
        };
        // The receiver should be another call expression to continue the chain
        let Expression::CallExpression(recv_call) = &member.object else {
            return count;
        };
        current = recv_call;
    }
}

fn is_outermost_chain_call<'a>(
    node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    // Walk up: our call -> StaticMemberExpression -> CallExpression
    let parent_id = nodes.parent_id(node_id);
    if parent_id == node_id {
        return true;
    }
    let parent = nodes.get_node(parent_id);
    // If the parent is a StaticMemberExpression, check if the grandparent is a qualifying call
    let AstKind::StaticMemberExpression(member) = parent.kind() else {
        return true;
    };
    let prop = member.property.name.as_str();
    if !CHAIN_METHODS.contains(&prop) {
        return true;
    }
    // Check if grandparent is a CallExpression
    let gp_id = nodes.parent_id(parent_id);
    if gp_id == parent_id {
        return true;
    }
    let gp = nodes.get_node(gp_id);
    !matches!(gp.kind(), AstKind::CallExpression(_))
}

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

        // Only consider calls whose method is a qualifying one.
        if method_name_of_call(call).is_none() {
            return;
        }
        if !is_outermost_chain_call(node.id(), semantic) {
            return;
        }
        let len = chain_length(call);
        if len < 3 {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{len} chained `.filter`/`.map`/`.reduce` calls — collapse into a \
                 single pass to avoid intermediate arrays."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
