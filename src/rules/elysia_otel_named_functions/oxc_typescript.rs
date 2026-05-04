//! elysia-otel-named-functions oxc backend — flag arrow functions in
//! `.derive`/`.resolve` under opentelemetry.

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
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop = member.property.name.as_str();
        if prop != "derive" && prop != "resolve" {
            return;
        }

        // The handler is the last argument.
        let Some(last_arg) = call.arguments.last() else { return };
        let Some(expr) = last_arg.as_expression() else { return };
        if !matches!(expr, Expression::ArrowFunctionExpression(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Arrow function in `.derive`/`.resolve` \u{2014} OpenTelemetry spans will be unnamed; use a named function.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
