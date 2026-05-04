//! react-no-access-state-in-setstate oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["setState"])
    }

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

        // Check callee is `this.setState`.
        let Expression::StaticMemberExpression(callee_member) = &call.callee else {
            return;
        };
        let Expression::ThisExpression(_) = &callee_member.object else {
            return;
        };
        if callee_member.property.name.as_str() != "setState" {
            return;
        }

        // Check arguments region for `this.state` using source text.
        let args_start = call.arguments.first().map(|a| a.span().start as usize);
        let args_end = call.arguments.last().map(|a| a.span().end as usize);
        if let (Some(start), Some(end)) = (args_start, args_end) {
            if end <= ctx.source.len() {
                let args_text = &ctx.source[start..end];
                if args_text.contains("this.state") {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "`this.state` inside `setState()` reads stale \
                                  state. Use the updater callback: \
                                  `setState(prev => ...)`."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
    }
}
