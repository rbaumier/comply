//! boundary-condition OXC backend.
//!
//! Flags `arr[0]` or `arr[arr.length - 1]` reads without a length guard
//! or nullish fallback.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ComputedMemberExpression(member) = node.kind() else {
            return;
        };
        let source = ctx.source;

        // Only flag when object is a plain identifier or member expression chain
        let obj_text = expr_text(&member.object, source);
        match &member.object {
            Expression::Identifier(_) => {}
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => {}
            _ => return,
        }

        let is_first = is_zero_index(&member.expression, source);
        let is_last = !is_first && is_last_index(&member.expression, obj_text, source);
        if !is_first && !is_last {
            return;
        }

        // Skip assignment targets
        if is_assignment_target(node, semantic) {
            return;
        }

        // Skip if wrapped in `?? fallback` or `|| fallback`
        if has_nullish_or_logical_fallback(node, semantic) {
            return;
        }

        // Skip if inside an `if` whose condition mentions `.length`
        if has_length_guard_ancestor(node, semantic, source) {
            return;
        }

        let which = if is_first { "first" } else { "last" };
        let at_arg = if is_first { "0" } else { "-1" };
        let (line, column) = byte_offset_to_line_col(source, member.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "boundary-condition".into(),
            message: format!(
                "Unchecked access to the {which} element — on an empty array this is `undefined`. \
                 Guard with `if ({obj_text}.length)`, use `{obj_text}.at({at_arg})`, or add a `?? fallback`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn expr_text<'a>(expr: &'a Expression, source: &'a str) -> &'a str {
    let start = expr.span().start as usize;
    let end = expr.span().end as usize;
    &source[start..end]
}

fn is_zero_index(expr: &Expression, source: &str) -> bool {
    if let Expression::NumericLiteral(lit) = expr {
        let text = &source[lit.span.start as usize..lit.span.end as usize];
        return text == "0";
    }
    false
}

/// Check if index has shape `<object_text>.length - 1`.
fn is_last_index(expr: &Expression, object_text: &str, source: &str) -> bool {
    let Expression::BinaryExpression(bin) = expr else {
        return false;
    };
    if !matches!(bin.operator, BinaryOperator::Subtraction) {
        return false;
    }
    // Right must be `1`
    let Expression::NumericLiteral(right) = &bin.right else {
        return false;
    };
    let right_text = &source[right.span.start as usize..right.span.end as usize];
    if right_text != "1" {
        return false;
    }
    // Left must be `<object>.length`
    let Expression::StaticMemberExpression(left_member) = &bin.left else {
        return false;
    };
    if left_member.property.name.as_str() != "length" {
        return false;
    }
    let left_obj_text = expr_text(&left_member.object, source);
    left_obj_text == object_text
}

fn is_assignment_target(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let parent = nodes.get_node(parent_id);
    // The ComputedMemberExpression is wrapped in a MemberExpression parent
    // in AstKind, so check its parent for assignments
    match parent.kind() {
        AstKind::AssignmentExpression(assign) => {
            // Check the node span overlaps the left side
            let left_start = assign.left.span().start;
            let left_end = assign.left.span().end;
            let node_span = node.kind().span();
            node_span.start >= left_start && node_span.end <= left_end
        }
        _ => false,
    }
}

fn has_nullish_or_logical_fallback(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    for _ in 0..6 {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::ParenthesizedExpression(_) | AstKind::TSNonNullExpression(_) => {
                current_id = parent_id;
                continue;
            }
            AstKind::LogicalExpression(logical) => {
                if matches!(
                    logical.operator,
                    LogicalOperator::Coalesce | LogicalOperator::Or
                ) {
                    // Must be the left operand
                    let left_end = logical.left.span().end;
                    let node_span = node.kind().span();
                    if node_span.end <= left_end {
                        return true;
                    }
                }
                return false;
            }
            _ => return false,
        }
    }
    false
}

fn has_length_guard_ancestor(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        if let AstKind::IfStatement(if_stmt) = parent.kind() {
            let cond_text = &source[if_stmt.test.span().start as usize..if_stmt.test.span().end as usize];
            if cond_text.contains(".length") {
                return true;
            }
        }
        current_id = parent_id;
    }
}
