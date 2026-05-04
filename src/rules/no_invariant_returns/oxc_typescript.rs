//! no-invariant-returns OXC backend — flag functions that always return the
//! same literal value.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
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
        let (span_start, func_span) = match node.kind() {
            AstKind::Function(func) => {
                if func.body.is_none() {
                    return;
                }
                (func.span.start, func.span)
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                if arrow.expression {
                    return; // concise body — single expression
                }
                (arrow.span.start, arrow.span)
            }
            _ => return,
        };

        // Collect return-statement literal values that belong directly to this function
        let nodes = semantic.nodes();
        let mut literals: Vec<String> = Vec::new();

        for snode in nodes.iter() {
            let AstKind::ReturnStatement(ret) = snode.kind() else {
                continue;
            };
            // Span check
            if ret.span.start < func_span.start || ret.span.end > func_span.end {
                continue;
            }
            // Must belong directly to this function, not a nested one
            if nearest_function_span(snode.id(), nodes) != Some(func_span) {
                continue;
            }

            let Some(arg) = &ret.argument else {
                // bare `return;` — non-literal, bail
                return;
            };
            match literal_text(arg) {
                Some(text) => literals.push(text),
                None => return, // non-literal return — can't prove invariance
            }
        }

        if literals.len() < 2 {
            return;
        }

        let first = &literals[0];
        if !literals.iter().all(|l| l == first) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function always returns the same literal value \u{2014} consider using a constant instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

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

fn literal_text(expr: &Expression) -> Option<String> {
    match expr {
        Expression::NumericLiteral(n) => Some(
            n.raw
                .as_ref()
                .map_or_else(|| n.value.to_string(), |r| r.to_string()),
        ),
        Expression::StringLiteral(s) => Some(format!("\"{}\"", s.value)),
        Expression::BooleanLiteral(b) => Some(b.value.to_string()),
        Expression::NullLiteral(_) => Some("null".into()),
        Expression::Identifier(id) if id.name.as_str() == "undefined" => {
            Some("undefined".into())
        }
        _ => None,
    }
}
