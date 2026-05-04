//! cyclomatic-complexity OXC backend — flag functions with complexity > threshold.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (span_start, func_span, name) = match node.kind() {
            AstKind::Function(func) => {
                let name = func
                    .id
                    .as_ref()
                    .map(|id| id.name.as_str())
                    .unwrap_or("<anonymous>");
                if func.body.is_none() {
                    return;
                }
                (func.span.start, func.span, name)
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                if arrow.expression {
                    return;
                }
                (arrow.span.start, arrow.span, "<anonymous>")
            }
            _ => return,
        };

        let threshold = ctx.config.threshold("cyclomatic-complexity", "max", ctx.lang);

        // Count branching nodes that belong directly to this function
        // (not to nested functions). Walk all semantic nodes whose span is
        // within ours and check ancestry.
        let mut complexity = 1usize;
        let nodes = semantic.nodes();
        for snode in nodes.iter() {
            // Quick span containment check
            let kind = snode.kind();
            let child_span = match kind {
                AstKind::IfStatement(s) => s.span,
                AstKind::ForStatement(s) => s.span,
                AstKind::ForInStatement(s) => s.span,
                AstKind::ForOfStatement(s) => s.span,
                AstKind::WhileStatement(s) => s.span,
                AstKind::DoWhileStatement(s) => s.span,
                AstKind::CatchClause(s) => s.span,
                AstKind::SwitchCase(s) => s.span,
                AstKind::ConditionalExpression(s) => s.span,
                AstKind::LogicalExpression(s) => s.span,
                _ => continue,
            };

            // Must be inside our function
            if child_span.start < func_span.start || child_span.end > func_span.end {
                continue;
            }

            // For LogicalExpression, only count &&, ||, ??
            if let AstKind::LogicalExpression(log) = kind {
                use oxc_ast::ast::LogicalOperator;
                if !matches!(
                    log.operator,
                    LogicalOperator::And | LogicalOperator::Or | LogicalOperator::Coalesce
                ) {
                    continue;
                }
            }

            // Check this node's nearest enclosing function is our node
            if nearest_function_span(snode.id(), nodes) != Some(func_span) {
                continue;
            }

            complexity += 1;
        }

        if complexity > threshold {
            let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Function `{name}` has a cyclomatic complexity of {complexity} (max: {threshold}).",
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Walk up ancestors to find the nearest enclosing function's span.
fn nearest_function_span(
    node_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> Option<oxc_span::Span> {
    for kind in nodes.ancestor_kinds(node_id).skip(1) {
        match kind {
            AstKind::Function(f) => return Some(f.span),
            AstKind::ArrowFunctionExpression(a) => return Some(a.span),
            _ => {}
        }
    }
    None
}
