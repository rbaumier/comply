//! nuxt-no-v-html-in-server backend.
//!
//! Vue SFC `<template>` blocks are not exposed to the TS grammar, but they
//! still arrive in `ctx.source` as raw text in `.vue` files and inline
//! template strings. We scan the file for `v-html=` occurrences with no
//! `DOMPurify` / `sanitize` mention nearby (same source) — a coarse but
//! cheap heuristic that catches the dangerous default usage.

use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { on ["program"] prefilter = ["v-html"] => |_node, _source, ctx, diagnostics|
    let src = ctx.source;
    if !is_nuxt_or_vue_source(src) {
        return;
    }
    let sanitized = ctx.source_contains("DOMPurify")
        || ctx.source_contains("sanitize(")
        || ctx.source_contains("sanitizeHtml(")
        || ctx.source_contains("purify(");
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
                path: ctx.path.to_path_buf().into(),
                line,
                column: 1,
                rule_id: "nuxt-no-v-html-in-server".into(),
                message: "`v-html` without DOMPurify/sanitize — XSS risk in SSR. Sanitize the value or render as components.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
        start = abs + 6;
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_unsanitized_v_html() {
        let src = "// <template><div v-html=\"raw\" /></template>\nimport {} from '#imports';";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn allows_v_html_with_dompurify() {
        let src = "// <template><div v-html=\"clean\" /></template>\nimport DOMPurify from 'dompurify';\nimport {} from '#imports';\nconst clean = DOMPurify.sanitize(raw);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_nuxt_files() {
        let src = "<div v-html=\"raw\" />";
        assert!(run_on(src).is_empty());
    }
}
