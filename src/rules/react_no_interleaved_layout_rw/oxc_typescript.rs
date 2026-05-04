//! OXC backend for react-no-interleaved-layout-rw.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const LAYOUT_READ_PROPS: &[&str] = &[
    "offsetWidth",
    "offsetHeight",
    "offsetTop",
    "offsetLeft",
    "clientWidth",
    "clientHeight",
    "scrollTop",
    "scrollLeft",
    "scrollWidth",
    "scrollHeight",
    "getBoundingClientRect",
    "getClientRects",
];

#[derive(Clone, Copy, PartialEq)]
enum Op {
    Read,
    Write,
}

pub struct Check;

fn is_interleaved(ops: &[Op]) -> bool {
    if ops.len() < 3 {
        return false;
    }
    let mut runs = 1;
    for w in ops.windows(2) {
        if w[0] != w[1] {
            runs += 1;
        }
    }
    runs >= 3
}

/// Collect layout reads and style writes from descendants of a function body,
/// skipping nested function scopes.
fn collect_ops(
    body_span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic,
    ops: &mut Vec<Op>,
) {
    // We need to track nested function scopes to skip them.
    // Collect nested function spans first.
    let mut nested_fn_spans: Vec<oxc_span::Span> = Vec::new();
    for n in semantic.nodes().iter() {
        let s = n.kind().span();
        if s.start <= body_span.start || s.end > body_span.end {
            continue;
        }
        match n.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                nested_fn_spans.push(s);
            }
            _ => {}
        }
    }

    // Now collect ops, skipping nested functions
    for n in semantic.nodes().iter() {
        let s = n.kind().span();
        if s.start < body_span.start || s.end > body_span.end {
            continue;
        }
        // Skip if inside a nested function
        if nested_fn_spans.iter().any(|fs| s.start >= fs.start && s.end <= fs.end) {
            continue;
        }
        match n.kind() {
            AstKind::StaticMemberExpression(member) => {
                let prop_name = member.property.name.as_str();
                if LAYOUT_READ_PROPS.contains(&prop_name) {
                    ops.push(Op::Read);
                }
            }
            AstKind::AssignmentExpression(assign) => {
                if let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(left) = &assign.left
                    && let Expression::StaticMemberExpression(obj_member) = &left.object
                        && obj_member.property.name.as_str() == "style" {
                            ops.push(Op::Write);
                        }
            }
            _ => {}
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::ArrowFunctionExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let body_span = match node.kind() {
            AstKind::Function(func) => {
                let Some(body) = &func.body else { return };
                body.span
            }
            AstKind::ArrowFunctionExpression(arrow) => arrow.body.span,
            _ => return,
        };

        let mut ops = Vec::new();
        collect_ops(body_span, semantic, &mut ops);
        if !is_interleaved(&ops) {
            return;
        }

        let span = node.kind().span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Layout reads (e.g. `offsetWidth`, `getBoundingClientRect`) interleaved \
                     with `.style.*` writes force sync layout. Batch reads first, writes second."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
