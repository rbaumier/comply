//! nuxt-no-v-html-in-server OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_nuxt_or_vue_source(src: &str) -> bool {
    src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("defineNuxtPlugin")
        || src.contains("defineNuxtRouteMiddleware")
        || src.contains("useNuxtApp")
        || src.contains("<template")
        || src.contains("defineComponent")
}

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["v-html"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !is_nuxt_or_vue_source(src) {
            return Vec::new();
        }
        let sanitized = ctx.source_contains("DOMPurify")
            || ctx.source_contains("sanitize(")
            || ctx.source_contains("sanitizeHtml(")
            || ctx.source_contains("purify(");

        let mut diagnostics = Vec::new();
        let mut start = 0;
        while let Some(pos) = src[start..].find("v-html") {
            let abs = start + pos;
            let prev = if abs == 0 { ' ' } else { src.as_bytes()[abs - 1] as char };
            if prev.is_alphanumeric() || prev == '_' || prev == '-' {
                start = abs + 6;
                continue;
            }
            if !sanitized {
                let line = src[..abs].matches('\n').count() + 1;
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`v-html` without DOMPurify/sanitize — XSS risk in SSR. Sanitize the value or render as components.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            start = abs + 6;
        }
        diagnostics
    }
}
