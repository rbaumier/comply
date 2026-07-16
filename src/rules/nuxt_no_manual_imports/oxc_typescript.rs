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
        // types are not). `import type { NuxtError } from 'nuxt/app'` — and the
        // equivalent all-inline-type `import { type A, type B }` — is always
        // required to compile — dropping it is not an option — so it is never a
        // redundant manual import. This also exempts the Nuxt framework's own
        // internals, which import their public types from `nuxt/app`.
        if is_type_only_import(import) {
            return;
        }

        // Importing from `#imports`/`#app`/`nuxt/app` is itself the Nuxt
        // marker — no separate file-level gate needed.
        let module = import.source.value.as_str();
        if module != "#imports" && module != "#app" && module != "nuxt/app" {
            return;
        }

        // A Nuxt *module*'s runtime library code (under a `runtime/` directory)
        // is compiled and shipped as library code; Nuxt auto-import is
        // unavailable at the module's own build time, so the explicit
        // `#imports`/`#app`/`nuxt/app` import is required and dropping it breaks
        // compilation.
        if crate::rules::path_utils::is_nuxt_module_runtime_file(ctx.path, ctx.project) {
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

/// Whether every binding this import introduces is an erased TypeScript type,
/// so nothing survives to runtime and it can never be a redundant Nuxt
/// auto-import.
///
/// True for a statement-level `import type { A } from '...'` and for the
/// all-inline-type form `import { type A, type B } from '...'` (every named
/// specifier carries its own `type` qualifier). A default or namespace binding,
/// or any value named specifier — including the mixed `import { a, type B }`,
/// which keeps `a` at runtime — is not type-only. A side-effect `import "pkg"`
/// (no specifiers) runs at runtime and is likewise not type-only.
fn is_type_only_import(import: &oxc_ast::ast::ImportDeclaration) -> bool {
    use oxc_ast::ast::ImportDeclarationSpecifier;

    if import.import_kind.is_type() {
        return true;
    }
    let Some(specifiers) = &import.specifiers else {
        return false;
    };
    !specifiers.is_empty()
        && specifiers.iter().all(|s| match s {
            ImportDeclarationSpecifier::ImportSpecifier(named) => named.import_kind.is_type(),
            ImportDeclarationSpecifier::ImportDefaultSpecifier(_)
            | ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => false,
        })
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

    /// Regression for issue #7550: an all-inline-type import
    /// (`import { type A, type B } from '#imports'`) is semantically identical to
    /// a statement-level `import type { A, B }` — every specifier is erased, so
    /// dropping it breaks the TypeScript build. It must not be flagged.
    #[test]
    fn allows_all_inline_type_import_issue_7550() {
        let src = "import { type CellRange, type Row } from '#imports';";
        assert!(run_on(src).is_empty(), "unexpected: {:?}", run_on(src));
    }

    /// A single all-inline-type specifier is likewise fully erased and required
    /// to compile.
    #[test]
    fn allows_single_inline_type_import_issue_7550() {
        let src = "import { type Foo } from '#imports';";
        assert!(run_on(src).is_empty(), "unexpected: {:?}", run_on(src));
    }

    /// Negative-space guard for issue #7550: the mixed form
    /// (`import { useFoo, type Bar }`) keeps a runtime value binding (`useFoo`)
    /// that Nuxt auto-imports, so the statement must STILL fire — only the
    /// all-inline-type case is exempt.
    #[test]
    fn still_flags_mixed_value_and_inline_type_import_issue_7550() {
        let src = "import { useFoo, type Bar } from '#imports';\nconst f = useFoo();";
        assert_eq!(run_on(src).len(), 1, "got: {:?}", run_on(src));
    }

    /// Negative-space guard for issue #7550: a namespace binding
    /// (`import * as app from '#app'`) is a runtime value, not an erased type, so
    /// the all-inline-type exemption must not swallow it — it must STILL fire.
    #[test]
    fn still_flags_namespace_import_issue_7550() {
        let src = "import * as app from '#app';\nconst a = app;";
        assert_eq!(run_on(src).len(), 1, "got: {:?}", run_on(src));
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

    /// Build a `ProjectCtx` rooted at a tempdir holding `package.json` with the
    /// given dependency map, plus the file under test on disk, and run the rule.
    /// Returns `(tempdir, diagnostics)` — keep the tempdir alive for the call.
    fn run_in_package(
        pkg_json: &str,
        rel_path: &str,
        source: &str,
    ) -> (tempfile::TempDir, Vec<Diagnostic>) {
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join(rel_path);
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, source).unwrap();
        let file_path = std::fs::canonicalize(&file_path).unwrap();
        let sf = SourceFile {
            path: file_path.clone(),
            language: Language::TypeScript,
        };
        let refs: Vec<&SourceFile> = vec![&sf];
        let project = ProjectCtx::load(&refs, &crate::config::Config::default());
        let file = crate::rules::file_ctx::FileCtx::build(
            &file_path,
            source,
            Language::TypeScript,
            &project,
        );
        let diags = crate::rules::test_helpers::run_oxc_check(
            &Check, source, &file_path, &project, &file,
        );
        (dir, diags)
    }

    /// Regression for issue #3300: a Nuxt *module*'s runtime composable
    /// (`src/runtime/composables/usePrefix.ts`) must import explicitly from
    /// `#imports` because auto-import is unavailable at the module's build time.
    /// The package depends on `@nuxt/kit`, so the rule must not flag it.
    #[test]
    fn allows_value_import_in_nuxt_module_runtime_issue_3300() {
        let pkg = r#"{"name":"@nuxt/ui","dependencies":{"@nuxt/kit":"^3.0.0"}}"#;
        let src = "import { useAppConfig } from '#imports';\nconst c = useAppConfig();\n";
        let (_dir, diags) = run_in_package(pkg, "src/runtime/composables/usePrefix.ts", src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    /// A Nuxt module's runtime *plugin* under `src/runtime/plugins/` is likewise
    /// library code; an explicit `#imports` value import there is required.
    #[test]
    fn allows_value_import_in_nuxt_module_runtime_plugin_issue_3300() {
        let pkg = r#"{"name":"@nuxt/ui","devDependencies":{"@nuxt/module-builder":"^0.8.0"}}"#;
        let src = "import { useState } from '#imports';\nconst s = useState('x');\n";
        let (_dir, diags) = run_in_package(pkg, "src/runtime/plugins/colors.ts", src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    /// Guard for issue #3300: in a Nuxt module package an explicit `#imports`
    /// value import OUTSIDE the `runtime/` directory (ordinary app-shaped source)
    /// must STILL fire — the exemption is scoped to runtime library code.
    #[test]
    fn still_flags_value_import_outside_runtime_in_nuxt_module_issue_3300() {
        let pkg = r#"{"name":"@nuxt/ui","dependencies":{"@nuxt/kit":"^3.0.0"}}"#;
        let src = "import { useAppConfig } from '#imports';\nconst c = useAppConfig();\n";
        let (_dir, diags) = run_in_package(pkg, "composables/useFoo.ts", src);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
    }

    /// Guard for issue #3300: a `runtime/` directory in a Nuxt *application*
    /// (no `@nuxt/kit`/`@nuxt/module-builder` dependency) is not module library
    /// code, so an explicit `#imports` value import there must STILL fire.
    #[test]
    fn still_flags_runtime_value_import_in_nuxt_app_issue_3300() {
        let pkg = r#"{"name":"my-app","dependencies":{"nuxt":"^3.0.0"}}"#;
        let src = "import { useAppConfig } from '#imports';\nconst c = useAppConfig();\n";
        let (_dir, diags) = run_in_package(pkg, "src/runtime/composables/usePrefix.ts", src);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
    }

    /// Regression for issue #4485: a Nuxt *module*'s `client/` directory (e.g.
    /// Nuxt DevTools' injected client-side app) is a separately-built bundle
    /// without auto-import, like `runtime/`. Its explicit `#imports` value import
    /// is required, so the rule must not flag it.
    #[test]
    fn allows_value_import_in_nuxt_module_client_issue_4485() {
        let pkg = r#"{"name":"@nuxt/devtools","dependencies":{"@nuxt/kit":"^3.0.0"}}"#;
        let src = "import { defineNuxtPlugin } from '#imports';\nexport default defineNuxtPlugin(() => {});\n";
        let (_dir, diags) = run_in_package(pkg, "client/plugins/floating-vue.ts", src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    /// Regression for issue #6412: a Nuxt *module*'s `runtime-utils/` directory
    /// (the `runtime-*` compiled-library family, e.g. `@nuxt/test-utils`) ships
    /// without auto-import, like `runtime/`. Its explicit `#imports` value import
    /// is required, so the rule must not flag it.
    #[test]
    fn allows_value_import_in_nuxt_module_runtime_utils_issue_6412() {
        let pkg = r#"{"name":"@nuxt/test-utils","dependencies":{"@nuxt/kit":"^3.0.0"}}"#;
        let src = "import { defineComponent, h, useRouter } from '#imports';\nexport default defineComponent({});\n";
        let (_dir, diags) =
            run_in_package(pkg, "src/runtime-utils/components/RouterLink.ts", src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    /// Guard for issue #6412: the `runtime-` prefix must be precise. An explicit
    /// `#imports` value import in a NON-runtime path of a Nuxt module package must
    /// STILL fire — the runtime-segment gate remains required.
    #[test]
    fn still_flags_value_import_outside_runtime_utils_in_nuxt_module_issue_6412() {
        let pkg = r#"{"name":"@nuxt/test-utils","dependencies":{"@nuxt/kit":"^3.0.0"}}"#;
        let src = "import { useAppConfig } from '#imports';\nconst c = useAppConfig();\n";
        let (_dir, diags) = run_in_package(pkg, "src/utils/foo.ts", src);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
    }

    /// Guard for issue #6412: the exemption matches the hyphenated `runtime-*`
    /// family, not any `runtime`-prefixed segment. A `runtimeconfig/` directory
    /// (no hyphen) is ordinary source, so an explicit `#imports` value import
    /// there must STILL fire.
    #[test]
    fn still_flags_value_import_in_runtimeconfig_in_nuxt_module_issue_6412() {
        let pkg = r#"{"name":"@nuxt/test-utils","dependencies":{"@nuxt/kit":"^3.0.0"}}"#;
        let src = "import { useAppConfig } from '#imports';\nconst c = useAppConfig();\n";
        let (_dir, diags) = run_in_package(pkg, "src/runtimeconfig/foo.ts", src);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
    }

    /// Guard for issue #4485: a `client/` directory in a Nuxt *application*
    /// (no `@nuxt/kit`/`@nuxt/module-builder` dependency) is not module build
    /// code, so an explicit `#imports` value import there must STILL fire.
    #[test]
    fn still_flags_client_value_import_in_nuxt_app_issue_4485() {
        let pkg = r#"{"name":"my-app","dependencies":{"nuxt":"^3.0.0"}}"#;
        let src = "import { defineNuxtPlugin } from '#imports';\nexport default defineNuxtPlugin(() => {});\n";
        let (_dir, diags) = run_in_package(pkg, "client/plugins/floating-vue.ts", src);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
    }
}
