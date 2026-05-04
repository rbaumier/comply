//! post-message-origin OxcCheck backend.

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

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["postMessage"])
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

        let is_post_message = match &call.callee {
            Expression::StaticMemberExpression(member) => {
                member.property.name.as_str() == "postMessage"
            }
            Expression::Identifier(ident) => ident.name.as_str() == "postMessage",
            _ => false,
        };

        if !is_post_message {
            return;
        }

        // postMessage(message, targetOrigin, [transfer])
        // Check second argument (targetOrigin)
        let is_unsafe = if call.arguments.len() < 2 {
            true // Missing targetOrigin
        } else {
            let arg = &call.arguments[1];
            match arg {
                oxc_ast::ast::Argument::StringLiteral(lit) => lit.value.as_str() == "*",
                oxc_ast::ast::Argument::TemplateLiteral(tpl) => {
                    tpl.expressions.is_empty()
                        && tpl.quasis.len() == 1
                        && tpl.quasis[0].value.raw.as_str() == "*"
                }
                _ => false,
            }
        };

        if !is_unsafe {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`postMessage()` with `'*'` or missing target origin — specify explicit origin."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
