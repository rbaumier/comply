//! prefer-array-to-sorted oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["sort"])
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

        // Callee must be `.sort(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "sort" {
            return;
        }

        let is_copy_pattern = match &member.object {
            // [...arr].sort()
            Expression::ArrayExpression(arr) => {
                arr.elements.len() == 1
                    && matches!(
                        arr.elements[0],
                        oxc_ast::ast::ArrayExpressionElement::SpreadElement(_)
                    )
            }
            // arr.slice().sort()
            Expression::CallExpression(inner_call) => {
                if let Expression::StaticMemberExpression(inner_member) = &inner_call.callee {
                    inner_member.property.name.as_str() == "slice"
                } else {
                    false
                }
            }
            _ => false,
        };

        if !is_copy_pattern {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `arr.toSorted()` instead of copying then sorting (ES2023).".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
