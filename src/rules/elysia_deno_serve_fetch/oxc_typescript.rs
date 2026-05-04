//! OxcCheck backend — flag `Deno.serve(app)` without `.fetch`.

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

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }
        // Callee must be `Deno.serve`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "serve" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "Deno" {
            return;
        }
        if call.arguments.is_empty() {
            return;
        }
        // If first arg is an object literal (config), handler is the second arg;
        // otherwise the handler is the first arg.
        let handler = if let Some(first) = call.arguments.first() {
            if let Some(Expression::ObjectExpression(_)) = first.as_expression() {
                call.arguments.get(1)
            } else {
                Some(first)
            }
        } else {
            return;
        };
        let Some(handler_arg) = handler else { return };
        let Some(expr) = handler_arg.as_expression() else { return };
        // Only flag bare identifiers (e.g. `app`), not `app.fetch`
        if !matches!(expr, Expression::Identifier(_)) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`Deno.serve(app)` does not call Elysia \u{2014} pass `app.fetch` instead.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
