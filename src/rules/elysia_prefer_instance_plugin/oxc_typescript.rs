//! elysia-prefer-instance-plugin OXC backend — flag callback-style Elysia plugins.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Returns true if the first parameter has a type annotation containing `Elysia`.
fn first_param_is_elysia(params: &oxc_ast::ast::FormalParameters, source: &str) -> bool {
    let Some(first) = params.items.first() else {
        return false;
    };
    let Some(ann) = &first.type_annotation else {
        return false;
    };
    let ann_text =
        &source[ann.span.start as usize..ann.span.end as usize];
    let trimmed = ann_text.trim_start_matches(':').trim();
    trimmed == "Elysia" || trimmed.starts_with("Elysia<")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ArrowFunctionExpression, AstType::Function]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let (params, span_start) = match node.kind() {
            AstKind::ArrowFunctionExpression(arrow) => (&arrow.params, arrow.span.start),
            AstKind::Function(func) => (&func.params, func.span.start),
            _ => return,
        };

        if !first_param_is_elysia(params, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Callback-style plugin `(app: Elysia) => ...` \u{2014} prefer `new Elysia({ name: '...' })` instance plugins for deduplication and type inference.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
