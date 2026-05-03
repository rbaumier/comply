//! OxcCheck backend for tanstack-query-require-stale-time — flag `new QueryClient(...)` without `staleTime`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["QueryClient"])
    }

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
        let Expression::Identifier(id) = &new_expr.callee else { return };
        if id.name.as_str() != "QueryClient" {
            return;
        }
        // Check if any argument contains "staleTime" in its source text.
        for arg in &new_expr.arguments {
            let arg_text = &ctx.source[arg.span().start as usize..arg.span().end as usize];
            if arg_text.contains("staleTime") {
                return;
            }
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`QueryClient` without a default `staleTime` refetches on every component mount.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
