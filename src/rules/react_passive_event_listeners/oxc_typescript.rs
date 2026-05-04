//! react-passive-event-listeners OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const SCROLL_EVENTS: &[&str] = &["touchstart", "touchmove", "wheel", "scroll"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["addEventListener"])
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

        // callee must be `*.addEventListener`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "addEventListener" {
            return;
        }

        // First argument must be a scroll/touch event string.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let event_name = match first_arg.as_expression() {
            Some(Expression::StringLiteral(lit)) => lit.value.as_str(),
            _ => return,
        };
        if !SCROLL_EVENTS.contains(&event_name) {
            return;
        }

        // If the callback calls preventDefault(), passive:true would break it — skip.
        if let Some(second_arg) = call.arguments.get(1) {
            let cb_src = &ctx.source
                [second_arg.span().start as usize..second_arg.span().end as usize];
            if cb_src.contains("preventDefault") {
                return;
            }
        }

        // Check if third argument contains `passive: true`.
        let has_passive = if let Some(third_arg) = call.arguments.get(2) {
            let opt_src =
                &ctx.source[third_arg.span().start as usize..third_arg.span().end as usize];
            opt_src.contains("passive: true") || opt_src.contains("passive:true")
        } else {
            false
        };

        if !has_passive {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Add `{{ passive: true }}` to `addEventListener('{event_name}', ...)` to avoid jank."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
