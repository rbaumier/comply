//! OXC backend for prefer-array-to-reversed.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use oxc_ast::ast::{ArrayExpressionElement, Expression};

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["reverse"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `obj.reverse`.
        let Expression::StaticMemberExpression(mem) = &call.callee else {
            return;
        };
        if mem.property.name.as_str() != "reverse" {
            return;
        }

        let is_copy_pattern = match &mem.object {
            // [...arr].reverse()
            Expression::ArrayExpression(arr) => {
                arr.elements.len() == 1
                    && matches!(arr.elements.first(), Some(ArrayExpressionElement::SpreadElement(_)))
            }
            // arr.slice().reverse()
            Expression::CallExpression(inner_call) => {
                if let Expression::StaticMemberExpression(inner_mem) = &inner_call.callee {
                    inner_mem.property.name.as_str() == "slice"
                } else {
                    false
                }
            }
            _ => false,
        };

        if !is_copy_pattern {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `arr.toReversed()` instead of copying then reversing (ES2023).".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
