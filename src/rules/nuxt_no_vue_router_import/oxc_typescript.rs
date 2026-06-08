//! nuxt-no-vue-router-import oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_nuxt_source(src: &str) -> bool {
    src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("defineNuxtConfig")
        || src.contains("defineNuxtPlugin")
        || src.contains("defineNuxtRouteMiddleware")
        || src.contains("useNuxtApp")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["vue-router"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };
        if !is_nuxt_source(ctx.source) {
            return;
        }
        if import.source.value.as_str() != "vue-router" {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use Nuxt's auto-imported `useRouter()` / `useRoute()` instead of importing `vue-router`.".into(),
            severity: Severity::Warning,
            span: Some((import.span.start as usize, (import.span.end - import.span.start) as usize)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_vue_router_import_in_nuxt_file() {
        let src = "import { useRouter } from 'vue-router';\nconst plugin = defineNuxtPlugin(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_other_imports() {
        let src = "import { ref } from 'vue';\nconst plugin = defineNuxtPlugin(() => {});";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_nuxt_files() {
        let src = "import { useRouter } from 'vue-router';";
        assert!(run_on(src).is_empty());
    }
}
