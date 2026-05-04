//! react-no-unwrapped-localstorage oxc backend.
//!
//! Flags every `localStorage.<method>` member access whose ancestor
//! chain does not include a `TryStatement`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["localStorage"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };

        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name != "localStorage" {
            return;
        }

        // Walk ancestors — if any is a TryStatement body, skip.
        for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
            if let AstKind::TryStatement(try_stmt) = ancestor.kind() {
                // Make sure we are inside the try block body, not catch/finally.
                let body_span = try_stmt.block.span();
                let node_start = member.span.start;
                if node_start >= body_span.start && node_start < body_span.end {
                    return;
                }
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`localStorage` access outside a `try`/`catch` — throws in private-browsing mode, \
                     SSR, or on quota errors. Wrap in `try { ... } catch { ... }`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
