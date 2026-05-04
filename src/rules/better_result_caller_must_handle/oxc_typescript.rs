//! better-result-caller-must-handle OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["better-result"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !super::imports_better_result(ctx.source) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let callee_text =
            &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if !super::returns_result(callee_text) {
            return;
        }

        // Only flag if the call is an expression statement (result is ignored).
        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::ExpressionStatement(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Returned Result from `{callee_text}(...)` is ignored \u{2014} assign, match, map, unwrap, or yield* it."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
