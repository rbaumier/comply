//! nuxt-no-vue-router-import oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, source_contains};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_nuxt_source(src: &str) -> bool {
    source_contains(src, "#imports")
        || source_contains(src, "nuxt/app")
        || source_contains(src, "#app")
        || source_contains(src, "defineNuxtConfig")
        || source_contains(src, "defineNuxtPlugin")
        || source_contains(src, "defineNuxtRouteMiddleware")
        || source_contains(src, "useNuxtApp")
}

/// True when the file exports a binding named `useRouter` or `useRoute`. Such a
/// file is the Nuxt framework source that *defines* the composable by wrapping
/// the `vue-router` types (`export const useRouter: typeof _useRouter = ...`),
/// so its `vue-router` import is required to implement the wrapper, not a misuse
/// of the auto-import. A Nuxt *application* file consumes the auto-imported
/// `useRouter()`/`useRoute()` and never exports a binding of that name.
///
/// Covers `export const`/`export function`/`export { ... }` value exports;
/// type-only exports (`export { type useRoute }`) are ignored since they bind no
/// runtime composable.
fn file_defines_router_composable(semantic: &oxc_semantic::Semantic<'_>) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{BindingPattern, Declaration, ModuleExportName};

    fn is_router_composable_name(name: &str) -> bool {
        name == "useRouter" || name == "useRoute"
    }

    semantic.nodes().iter().any(|node| {
        let AstKind::ExportNamedDeclaration(export) = node.kind() else {
            return false;
        };
        if export.export_kind.is_type() {
            return false;
        }
        if let Some(declaration) = &export.declaration {
            return match declaration {
                Declaration::VariableDeclaration(var) => var.declarations.iter().any(|decl| {
                    matches!(&decl.id, BindingPattern::BindingIdentifier(id)
                        if is_router_composable_name(id.name.as_str()))
                }),
                Declaration::FunctionDeclaration(func) => func
                    .id
                    .as_ref()
                    .is_some_and(|id| is_router_composable_name(id.name.as_str())),
                _ => false,
            };
        }
        export.specifiers.iter().any(|spec| {
            !spec.export_kind.is_type()
                && match &spec.exported {
                    ModuleExportName::IdentifierName(id) => {
                        is_router_composable_name(id.name.as_str())
                    }
                    ModuleExportName::IdentifierReference(id) => {
                        is_router_composable_name(id.name.as_str())
                    }
                    ModuleExportName::StringLiteral(lit) => {
                        is_router_composable_name(lit.value.as_str())
                    }
                }
        })
    })
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };
        if import.source.value.as_str() != "vue-router" {
            return;
        }
        if !is_nuxt_source(ctx.source) {
            return;
        }
        if file_defines_router_composable(semantic) {
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    /// Regression for issue #1581: the Nuxt framework's own `router.ts` imports
    /// `vue-router` types to *define* the `useRouter`/`useRoute` wrappers, and
    /// references `useNuxtApp` inside them. It exports the composables, so it is
    /// framework source, not auto-import misuse — it must not be flagged.
    #[test]
    fn allows_nuxt_framework_file_defining_router_composables_issue_1581() {
        let src = "import type { Router, useRoute as _useRoute, useRouter as _useRouter } from 'vue-router';\n\
            import { useNuxtApp } from '../nuxt';\n\
            export const useRouter: typeof _useRouter = () => useNuxtApp()?.$router as Router;\n\
            export const useRoute: typeof _useRoute = () => useNuxtApp()._route;";
        assert!(run_on(src).is_empty(), "unexpected: {:?}", run_on(src));
    }

    /// The wrapper may be defined with `export function` rather than
    /// `export const`.
    #[test]
    fn allows_function_declared_router_composable() {
        let src = "import type { Router } from 'vue-router';\n\
            export function useRouter(): Router { return useNuxtApp().$router as Router; }";
        assert!(run_on(src).is_empty(), "unexpected: {:?}", run_on(src));
    }

    /// Negative-space guard: an ordinary Nuxt *application* file that calls the
    /// auto-imported `useNuxtApp()` and genuinely imports from `vue-router` does
    /// not export a `useRouter`/`useRoute` binding, so it must STILL fire.
    #[test]
    fn still_flags_user_file_consuming_nuxt_and_importing_vue_router() {
        let src = "import { useRouter } from 'vue-router';\n\
            const app = useNuxtApp();\n\
            export function setup() { const r = useRouter(); return r; }";
        assert_eq!(run_on(src).len(), 1, "got: {:?}", run_on(src));
    }
}
