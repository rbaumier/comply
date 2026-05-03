//! better-auth-drizzle-useplural oxc backend — require `usePlural: true` when
//! a `users` table is used with `drizzleAdapter`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
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

        let Expression::Identifier(id) = &call.callee else { return };
        if id.name.as_str() != "drizzleAdapter" {
            return;
        }

        // Look for an object argument.
        let Some(obj_arg) = call.arguments.iter().find_map(|arg| {
            if let Some(Expression::ObjectExpression(_)) = arg.as_expression() {
                Some(arg)
            } else {
                None
            }
        }) else {
            return;
        };

        // Check the object text for `users` identifier (not in strings).
        let obj_text = &ctx.source[obj_arg.span().start as usize..obj_arg.span().end as usize];

        // Only flag if `users` appears as an identifier reference.
        if !obj_text.contains("users") {
            return;
        }

        // Check if `usePlural: true` is present.
        if obj_text.contains("usePlural: true") || obj_text.contains("usePlural:true") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`drizzleAdapter` uses a plural `users` table — add `usePlural: true`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
