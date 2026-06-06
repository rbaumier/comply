//! OXC backend for elysia-nextjs-typeof-process.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UnaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@elysiajs/eden"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UnaryExpression(unary) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !ctx.source_contains("@elysiajs/eden") {
            return;
        }

        if unary.operator != oxc_ast::ast::UnaryOperator::Typeof {
            return;
        }

        let text = &ctx.source[unary.span.start as usize..unary.span.end as usize];
        if !text.starts_with("typeof window") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, unary.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `typeof process` instead of `typeof window` — `window` checks misclassify edge / RSC runtimes.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
