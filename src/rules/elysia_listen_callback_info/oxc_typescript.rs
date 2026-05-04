//! elysia-listen-callback-info oxc backend — flag .listen() with no callback.

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

        // Callee must be `*.listen`.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "listen" {
            return;
        }

        // Must have exactly one argument.
        if call.arguments.len() != 1 {
            return;
        }

        // If the single arg is already a callback, don't flag.
        let Some(expr) = call.arguments[0].as_expression() else { return };
        if matches!(
            expr,
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
        ) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.listen(...)` has no callback — pass one and log the server info so deploys show where the server is bound.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
