//! node-no-sync OXC backend — flag synchronous Node.js method calls (`*Sync()`).

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
        Some(&["Sync"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if super::allows_sync_node_api(ctx.path, ctx.source) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let (method_name, full_name) = match &call.callee {
            oxc_ast::ast::Expression::Identifier(id) => {
                let name = id.name.as_str();
                (name, name.to_string())
            }
            oxc_ast::ast::Expression::StaticMemberExpression(member) => {
                let prop = member.property.name.as_str();
                let full =
                    &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
                (prop, full.to_string())
            }
            _ => return,
        };

        // Must end with "Sync" and have at least one char before it.
        if method_name.len() <= 4 || !method_name.ends_with("Sync") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Unexpected sync method: `{full_name}()`. Use the async variant instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
