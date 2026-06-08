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

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_imports_from_pound_imports() {
        let src = "import { useRuntimeConfig } from '#imports';\nconst cfg = useRuntimeConfig();\nconst plugin = defineNuxtPlugin(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_imports_from_pound_app() {
        let src = "import { useNuxtApp } from '#app';\nconst plugin = defineNuxtPlugin(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_other_imports_in_nuxt_file() {
        let src = "import { ref } from 'vue';\nconst plugin = defineNuxtPlugin(() => {});";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_nuxt_files() {
        // No Nuxt markers at all (no `#imports`, no `defineNuxtPlugin`, etc.).
        let src = "import { foo } from 'lodash';\nconst x = foo();";
        assert!(run_on(src).is_empty());
    }
}
