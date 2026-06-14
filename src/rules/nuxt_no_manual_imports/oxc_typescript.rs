//! OXC backend for nuxt-no-manual-imports.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

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
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };

        // Type-only imports bring in TypeScript types, which Nuxt never
        // auto-imports (auto-imports are runtime composables/components, erased
        // types are not). `import type { NuxtError } from 'nuxt/app'` is always
        // required to compile — dropping it is not an option — so it is never a
        // redundant manual import. This also exempts the Nuxt framework's own
        // internals, which import their public types from `nuxt/app`.
        if import.import_kind.is_type() {
            return;
        }

        // Importing from `#imports`/`#app`/`nuxt/app` is itself the Nuxt
        // marker — no separate file-level gate needed.
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
    fn flags_value_import_from_pound_app() {
        let src = "import { useNuxtApp } from '#app';";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_value_import_from_pound_imports() {
        let src = "import { useState } from '#imports';";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_unrelated_module() {
        let src = "import { ref } from 'vue';";
        assert!(run_on(src).is_empty());
    }

    /// Regression for issue #1583: the Nuxt framework's own source files import
    /// their public *types* from `nuxt/app` (`import type { NuxtError } from
    /// 'nuxt/app'`). Nuxt only auto-imports runtime composables, never types, so
    /// a type-only import can never be a redundant manual import — dropping it
    /// breaks compilation. It must not be flagged.
    #[test]
    fn allows_type_only_import_from_nuxt_app_issue_1583() {
        let src = "import type { NuxtError } from 'nuxt/app';";
        assert!(run_on(src).is_empty(), "unexpected: {:?}", run_on(src));
    }

    /// A type-only import from the `#app` virtual module is likewise required
    /// (types are not auto-imported).
    #[test]
    fn allows_type_only_import_from_pound_app() {
        let src = "import type { NuxtApp } from '#app';";
        assert!(run_on(src).is_empty(), "unexpected: {:?}", run_on(src));
    }

    /// Negative-space guard: an ordinary Nuxt *application* file that manually
    /// imports a runtime composable Nuxt auto-imports (a *value* import from
    /// `nuxt/app`) must STILL fire — that is the redundancy this rule exists to
    /// catch.
    #[test]
    fn still_flags_value_import_from_nuxt_app() {
        let src = "import { useState } from 'nuxt/app';\nconst s = useState('x');";
        assert_eq!(run_on(src).len(), 1, "got: {:?}", run_on(src));
    }
}
