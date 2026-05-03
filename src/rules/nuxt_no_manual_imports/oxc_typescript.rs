//! OXC backend for nuxt-no-manual-imports.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_nuxt_source(src: &str) -> bool {
    src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("defineNuxtConfig")
        || src.contains("defineNuxtPlugin")
        || src.contains("defineNuxtRouteMiddleware")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_nuxt_source(ctx.source) {
            return;
        }

        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };

        let module = import.source.value.as_str();
        if module != "#imports" && module != "#app" && module != "nuxt/app" {
            return;
        }

        let start = import.span.start as usize;
        let len = (import.span.end - import.span.start) as usize;
        let (line, column) = byte_offset_to_line_col(ctx.source, start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Nuxt auto-imports composables from `#imports`/`#app` — drop the explicit import.".into(),
            severity: Severity::Warning,
            span: Some((start, len)),
        });
    }
}
