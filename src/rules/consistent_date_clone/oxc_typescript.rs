//! consistent-date-clone oxc backend — flag `new Date(date.getTime())`
//! and `new Date(date.valueOf())` → use `new Date(date)` directly.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        // Constructor must be `Date`.
        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "Date" {
            return;
        }

        // Must have exactly one argument.
        if new_expr.arguments.len() != 1 {
            return;
        }

        // The argument must be a call expression: `expr.getTime()` or `expr.valueOf()`.
        let arg = &new_expr.arguments[0];
        let oxc_ast::ast::Argument::CallExpression(call) = arg else { return };

        // Callee must be a member expression.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method = member.property.name.as_str();
        if method != "getTime" && method != "valueOf" {
            return;
        }

        // Inner call must have no arguments.
        if !call.arguments.is_empty() {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unnecessary `.getTime()`/`.valueOf()` — use `new Date(date)` directly.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
