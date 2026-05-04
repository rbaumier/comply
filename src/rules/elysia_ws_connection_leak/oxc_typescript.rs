//! elysia-ws-connection-leak OXC backend — flag `.ws()` configs that add to a
//! Set in `open` but don't clean up on error/close.

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
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "ws" {
            return;
        }

        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

        // Need an `open(` handler that does `.add(`.
        if !norm.contains("open(") && !norm.contains("open:") {
            return;
        }
        if !args_text.contains(".add(") {
            return;
        }

        // No error handler, or error handler exists but lacks `.delete(`.
        let has_error = norm.contains("error(") || norm.contains("error:");
        let cleans_up = args_text.contains(".delete(");

        if has_error && cleans_up {
            return;
        }

        let msg = if !has_error {
            "`.ws()` `open` adds to a Set but no `error` handler is defined — dead sockets leak."
        } else {
            "`.ws()` `open` adds to a Set but `error`/`close` does not call `.delete(ws)` — dead sockets leak."
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: msg.into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
