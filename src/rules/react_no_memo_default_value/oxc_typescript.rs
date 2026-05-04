//! OXC backend for react-no-memo-default-value.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_memo_callee(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => id.name.as_str() == "memo",
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "React" && member.property.name.as_str() == "memo"
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Search the parameters span for assignment patterns with empty array/object defaults.
fn find_unstable_default(
    params_span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic,
) -> Option<(oxc_span::Span, &'static str)> {
    for node in semantic.nodes().iter() {
        let span = node.kind().span();
        if span.start < params_span.start || span.end > params_span.end {
            continue;
        }
        if let AstKind::AssignmentPattern(pat) = node.kind() {
            match &pat.right {
                Expression::ArrayExpression(arr) if arr.elements.is_empty() => {
                    return Some((arr.span, "array"));
                }
                Expression::ObjectExpression(obj) if obj.properties.is_empty() => {
                    return Some((obj.span, "object"));
                }
                _ => {}
            }
        }
    }
    None
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
        if !is_memo_callee(&call.callee) {
            return;
        }

        // Find the inline component function among the arguments
        let params_span = call.arguments.iter().find_map(|arg| {
            let expr = arg.as_expression()?;
            match expr {
                Expression::ArrowFunctionExpression(arrow) => Some(arrow.params.span),
                Expression::FunctionExpression(func) => Some(func.params.span),
                _ => None,
            }
        });
        let Some(params_span) = params_span else {
            return;
        };

        let Some((default_span, kind)) = find_unstable_default(params_span, semantic) else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, default_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Default `{kind}` value inside `memo(...)` creates a new reference every render — extract to a module-level constant."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
