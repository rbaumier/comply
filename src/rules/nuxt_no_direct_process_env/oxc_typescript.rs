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

        // Build/tooling config files (`nuxt.config.ts`, `vite.config.ts`, ...) are
        // evaluated by Node at build time and never shipped to the browser, so reading
        // `process.env` there is the canonical way to access build/CI env — not a
        // client-runtime mistake.
        if crate::rules::path_utils::is_config_file(ctx.path) {
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    // #4434 — `nuxt.config.ts` is build-time config evaluated by Node; reading
    // `process.env` there is canonical, not a client-runtime mistake.
    #[test]
    fn allows_process_env_in_nuxt_config() {
        let src = "export default defineNuxtConfig({\n  modules: [\n    ...(process.env.CI ? [] : ['../local']),\n  ],\n});";
        assert!(run_on_path(src, "nuxt.config.ts").is_empty());
    }

    #[test]
    fn allows_process_env_in_nested_nuxt_config() {
        let src = "export default defineNuxtConfig({\n  modules: [\n    ...(process.env.CI ? [] : ['../local']),\n  ],\n});";
        assert!(run_on_path(src, "playgrounds/tab-seo/nuxt.config.ts").is_empty());
    }

    // Load-bearing negative: non-config Nuxt source (a plugin) must still be
    // flagged so the fix only suppresses config files, not all Nuxt code.
    #[test]
    fn still_flags_process_env_in_nuxt_plugin() {
        let src = "export default defineNuxtPlugin(() => {\n  const id = process.env.FOO;\n});";
        let d = run_on_path(src, "plugins/analytics.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("process.env"));
    }
}
