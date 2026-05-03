//! prefer-timer-args — OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["setTimeout", "setInterval"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::Identifier(callee_id) = &call.callee else { return };
        let func_name = callee_id.name.as_str();
        if func_name != "setTimeout" && func_name != "setInterval" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        let Argument::ArrowFunctionExpression(arrow) = first_arg else { return };

        // Only flag expression-body arrows: () => fn(args)
        if !arrow.expression {
            return;
        }

        // The body should have exactly one statement which is an expression statement
        // containing a call expression with an identifier callee (not a method call).
        let Some(stmt) = arrow.body.statements.first() else { return };
        let Statement::ExpressionStatement(expr_stmt) = stmt else { return };
        let Expression::CallExpression(inner_call) = &expr_stmt.expression else { return };
        let Expression::Identifier(_) = &inner_call.callee else { return };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Pass arguments directly to `{func_name}` instead of wrapping in arrow function."),
            severity: Severity::Warning,
            span: None,
        });
    }
}
