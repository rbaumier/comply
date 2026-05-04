//! OxcCheck backend for elysia-inline-handlers.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options",
];

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
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&prop) {
            return;
        }

        // Need at least 2 args: path + handler.
        if call.arguments.len() < 2 {
            return;
        }

        let Some(handler_expr) = call.arguments[1].as_expression() else { return };
        match handler_expr {
            // Inline handlers are fine.
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => return,
            // Literals are fine (static responses).
            Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::ObjectExpression(_)
            | Expression::ArrayExpression(_)
            | Expression::TemplateLiteral(_) => return,
            // Identifier or member expression = handler by reference.
            Expression::Identifier(_) | Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_) => {}
            _ => return,
        }

        let handler_span = handler_expr.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, handler_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route handler passed by reference loses Elysia's type inference. Wrap in an inline arrow function.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
