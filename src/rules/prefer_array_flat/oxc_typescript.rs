//! OXC backend for prefer-array-flat.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_empty_array(expr: &Expression) -> bool {
    matches!(expr, Expression::ArrayExpression(arr) if arr.elements.is_empty())
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
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

        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();

        match method {
            "concat" => {
                // [].concat(...arr)
                if !is_empty_array(&member.object) {
                    return;
                }
                // First argument should be a spread element.
                let Some(first) = call.arguments.first() else {
                    return;
                };
                if !matches!(first, Argument::SpreadElement(_)) {
                    return;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer `.flat()` over legacy array flattening patterns.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            "reduce" => {
                // .reduce((a, b) => a.concat(b), []) or .reduce((a, b) => [...a, ...b], [])
                if call.arguments.len() != 2 {
                    return;
                }
                let Some(init_expr) = call.arguments.get(1).and_then(|a| a.as_expression()) else {
                    return;
                };
                if !is_empty_array(init_expr) {
                    return;
                }

                // Callback body should contain `.concat(` or `[...`
                let Some(cb_expr) = call.arguments[0].as_expression() else {
                    return;
                };
                let cb_span = cb_expr.span();
                let cb_text = &ctx.source[cb_span.start as usize..cb_span.end as usize];
                if !cb_text.contains(".concat(") && !cb_text.contains("[...") {
                    return;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer `.flat()` over legacy array flattening patterns.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
