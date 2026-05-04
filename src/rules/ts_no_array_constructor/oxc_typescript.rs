//! OXC backend for ts-no-array-constructor.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_array_identifier(expr: &oxc_ast::ast::Expression) -> bool {
    match expr {
        oxc_ast::ast::Expression::Identifier(id) => id.name == "Array",
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::CallExpression(call) => {
                if !is_array_identifier(&call.callee) {
                    return;
                }
                // Skip if type arguments present.
                if call.type_arguments.is_some() {
                    return;
                }
                // Skip single-argument calls.
                if call.arguments.len() == 1 {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Use array literal `[]` instead of `Array()` constructor.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::NewExpression(new_expr) => {
                if !is_array_identifier(&new_expr.callee) {
                    return;
                }
                if new_expr.type_arguments.is_some() {
                    return;
                }
                if new_expr.arguments.len() == 1 {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Use array literal `[]` instead of `Array()` constructor.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
