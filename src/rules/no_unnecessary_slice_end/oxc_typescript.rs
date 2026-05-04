//! no-unnecessary-slice-end oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_unnecessary_end(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed == "Infinity"
        || trimmed == "Number.POSITIVE_INFINITY"
        || trimmed.ends_with(".length")
}

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be a member expression with property "slice".
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "slice" {
            return;
        }

        // Must have exactly 2 arguments.
        if call.arguments.len() != 2 {
            return;
        }

        let second = &call.arguments[1];
        let second_text =
            &ctx.source[second.span().start as usize..second.span().end as usize];

        if is_unnecessary_end(second_text) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "The `end` argument is unnecessary \u{2014} `.slice(start)` already goes to the end.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
