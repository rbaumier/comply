use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, source_contains};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

fn is_nuxt_source(src: &str) -> bool {
    source_contains(src, "#imports")
        || source_contains(src, "nuxt/app")
        || source_contains(src, "#app")
        || source_contains(src, "defineNuxtConfig")
        || source_contains(src, "defineNuxtPlugin")
        || source_contains(src, "defineNuxtRouteMiddleware")
        || source_contains(src, "useRuntimeConfig")
        || source_contains(src, "useNuxtApp")
}

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["process"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StaticMemberExpression(member) = node.kind() else { return };

        let full_span = member.span;
        let full_text = &ctx.source[full_span.start as usize..full_span.end as usize];
        if full_text != "process.env" && !full_text.starts_with("process.env.") {
            return;
        }

        let is_process = matches!(&member.object, Expression::Identifier(id) if id.name == "process");
        if !is_process {
            return;
        }

        if !is_nuxt_source(ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, full_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`process.env` is unavailable on the client; use `useRuntimeConfig()` instead.".into(),
            severity: Severity::Error,
            span: Some((full_span.start as usize, full_span.size() as usize)),
        });
    }
}
