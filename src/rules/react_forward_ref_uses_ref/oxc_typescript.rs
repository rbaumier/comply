//! react-forward-ref-uses-ref oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["forwardRef"])
    }

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

        // Check callee is `forwardRef` or `React.forwardRef`.
        let is_forward_ref = match &call.callee {
            Expression::Identifier(id) => id.name == "forwardRef",
            Expression::StaticMemberExpression(m) => {
                if let Expression::Identifier(obj) = &m.object {
                    obj.name == "React" && m.property.name.as_str() == "forwardRef"
                } else {
                    false
                }
            }
            _ => false,
        };
        if !is_forward_ref {
            return;
        }

        // Get the first argument (the render function).
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };

        let param_count = match expr {
            Expression::ArrowFunctionExpression(arrow) => arrow.params.items.len(),
            Expression::FunctionExpression(func) => func.params.items.len(),
            _ => return,
        };

        if param_count < 2 {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`forwardRef` component is missing the `ref` parameter.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
