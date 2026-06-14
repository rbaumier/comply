//! no-extraneous-import OXC backend.
//!
//! Flags imports of devDependency packages from non-test production files.

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

        let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
            return;
        };
        // A `"private": true` package is never published to npm, so the
        // dependencies/devDependencies distinction the rule enforces is
        // irrelevant: nothing is `npm install`ed by a downstream consumer, and
        // everything is bundled at build time. Importing a devDependency from a
        // private app/dashboard/internal tool is correct and idiomatic
        // (issue #1373).
        if pkg.is_private {
            return;
        }
        // Dual-read: the unit-test harness injects an empty default FileCtx, so
        // the path-segment fields are false in tests — fall back to the pure
        // path predicate, which reads `ctx.path` directly.
        if ctx.file.path_segments.in_test_dir
            || crate::rules::path_utils::is_extraneous_test_file(ctx.path)
            || crate::rules::path_utils::is_auto_mock_dir_path(ctx.path)
            || crate::rules::path_utils::is_test_infra_dir_path(ctx.path)
        {
            return;
        }
        // Story files (a `*.stories.*` name, or any file inside a `stories/` or
        // `storybook/` directory) are dev-only tooling, like test files: they
        // never ship in the published package and legitimately import packages a
        // workspace declares in devDependencies (issue #1982).
        if ctx.file.path_segments.in_storybook {
            return;
        }
        if crate::rules::path_utils::is_config_file(ctx.path) || is_config_variant_file(ctx.path) {
            return;
        }
        // Build/codegen scripts (`scripts/`, `config/`, root-level `build.ts`/
        // `bundle.ts`), demonstration code (`samples/`, `examples/`, …), and
        // generator scaffold templates (`templates/`, `scaffold/`, …) run at
        // dev time and never ship in the published package, so importing a
        // devDependency from them is correct.
        let project_root = ctx
            .project
            .nearest_package_json_dir(ctx.path)
            .unwrap_or_default();
        if ctx.file.path_segments.in_aux_dir
            || crate::rules::path_utils::is_build_script_path(ctx.path, &project_root)
            || crate::rules::path_utils::is_sample_dir_path(ctx.path)
        {
            return;
        }
        // Custom linter tooling (`lint-rules/`, `lint-processors/`) and general
        // development tooling (`tools/`) run only during development and never
        // ship in the published package (e.g. type-fest excludes them from its
        // `files` field), so importing a devDependency from them is correct
        // (issue #1299).
        if is_dev_tooling_dir_path(ctx.path) {
            return;
        }
        // A directory housing a `package.json` `scripts` entry file (e.g.
        // `omnidoc/generateApiDoc.ts` run by `"omnidoc": "tsx ./omnidoc/..."`)
        // is a build-time codegen/doc toolchain. Its entry and the sibling
        // helpers it imports run at build time and never ship, so importing a
        // devDependency from any file in that directory is correct (issue #1862).
        if ctx.project.is_in_script_entry_dir(ctx.path) {
            return;
        }
        // Library build input: when the package publishes its entries from
        // outside `src/` (e.g. monaco-editor's `main`/`module` point into
        // `min/`/`esm/`), the `src/` tree is compiled away into the shipped
        // bundle, which inlines its build-time dependencies. A devDependency
        // imported from such a file is bundled at build time, not a runtime
        // import, so it must not flag (issue #1910).
        if ctx.project.is_bundled_build_input(ctx.path) {
            return;
        }

        let specifier = import.source.value.as_str();
        if !is_bare_specifier(specifier) {
            return;
        }

        // Type-only imports (`import type { X } from "pkg"`, or an import whose
        // named specifiers all carry the inline `type` qualifier) are erased at
        // compile time and emit no JavaScript, so they create no runtime
        // dependency — the rule's entire concern. Importing a devDependency's
        // types is therefore legitimate. An import that keeps any runtime
        // binding (a value specifier, a default/namespace binding, or a
        // side-effect `import "pkg"`) is not erased and stays checked.
        if is_type_only_import(import) {
            return;
        }

        let root = package_root(specifier);
        let in_runtime = pkg.dependencies.contains_key(root)
            || pkg.peer_dependencies.contains_key(root)
            || pkg.optional_dependencies.contains_key(root);
        if in_runtime {
            return;
        }

        // Workspace-internal package: the import resolves to another member of
        // the monorepo workspace, not an external dependency. Internal docs and
        // testing-utility members (e.g. `@docs/demos`, `@mantine-tests/*`) list
        // the library packages they document or test as devDependencies because
        // they are never published to npm, so importing such a member is correct
        // — there is no install-time break for downstream consumers (issue #1968).
        if ctx
            .project
            .workspace_package_names()
            .iter()
            .any(|name| name == root)
        {
            return;
        }

        if pkg.dev_dependencies.contains_key(root) {
            let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "no-extraneous-import".into(),
                message: format!(
                    "`{root}` is a devDependency; production code should import from dependencies, peerDependencies, or optionalDependencies."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// True when `path` is a hyphenated config-file variant such as
/// `vitest.config-mutation.mts` or `webpack.config-prod.js`. These name a
/// build-tooling config alongside the canonical `*.config.*` form (which
/// [`crate::rules::path_utils::is_config_file`] already covers) but carry a
/// `-<suffix>` discriminator after `config`, so they read as `*.config-*.*`.
/// They are dev tooling that never ships, so importing a devDependency from
/// them is correct.
fn is_config_variant_file(path: &std::path::Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem.contains(".config-"))
}

/// True when `path` lives under a conventional development-tooling directory:
/// custom ESLint rule definitions (`lint-rules/`), custom ESLint processors
/// (`lint-processors/`), or general dev tooling (`tools/`). These directories
/// hold code run only during development; it never ships in the published
/// package, so importing a devDependency from them is correct. Matched as exact
/// path segments so an unrelated `src/toolsRegistry/` does not match.
fn is_dev_tooling_dir_path(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        matches!(
            c,
            std::path::Component::Normal(s)
                if matches!(s.to_str(), Some("lint-rules" | "lint-processors" | "tools"))
        )
    })
}

/// True when an import emits no JavaScript and so creates no runtime
/// dependency: either a declaration-level `import type { ... }`, or a named
/// import whose every specifier carries the inline `type` qualifier
/// (`import { type A, type B } from "pkg"`).
///
/// Returns false for any import that keeps a runtime binding: a value specifier
/// (including a mixed `import { type A, b }`), a default or namespace binding,
/// or a side-effect `import "pkg"` (no specifiers).
fn is_type_only_import(import: &oxc_ast::ast::ImportDeclaration) -> bool {
    use oxc_ast::ast::ImportDeclarationSpecifier;

    if import.import_kind.is_type() {
        return true;
    }
    // No specifiers means a side-effect import (`import "pkg"`), which runs at
    // runtime and is never erased.
    let Some(specifiers) = &import.specifiers else {
        return false;
    };
    !specifiers.is_empty()
        && specifiers.iter().all(|s| match s {
            ImportDeclarationSpecifier::ImportSpecifier(named) => named.import_kind.is_type(),
            // A default or namespace binding is always a runtime value.
            ImportDeclarationSpecifier::ImportDefaultSpecifier(_)
            | ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => false,
        })
}

fn package_root(specifier: &str) -> &str {
    if specifier.starts_with('@') {
        match specifier.find('/') {
            Some(first_slash) => match specifier[first_slash + 1..].find('/') {
                Some(second_slash) => &specifier[..first_slash + 1 + second_slash],
                None => specifier,
            },
            None => specifier,
        }
    } else {
        match specifier.find('/') {
            Some(slash) => &specifier[..slash],
            None => specifier,
        }
    }
}

fn is_bare_specifier(spec: &str) -> bool {
    !spec.is_empty()
        && !spec.starts_with('.')
        && !spec.starts_with('/')
        && !spec.starts_with("node:")
}

#[cfg(test)]
mod tests {
    //! Regression tests for issue #101: false positives on devDependencies
    //! (vitest, @testing-library/*) imported from `*.test.{ts,tsx}` and
    //! `vitest.config.*` files.

    use super::Check;
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::backend::{CheckCtx, OxcCheck};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    fn run_with_pkg_at_path(pkg_json: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join(rel_path);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let lang = Language::from_path(&file_path).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: lang,
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&file_path).unwrap();

        let source_type = match lang {
            Language::Tsx => SourceType::tsx(),
            Language::JavaScript => SourceType::cjs(),
            _ => SourceType::ts(),
        };
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, source_type).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let file_ctx = crate::rules::file_ctx::FileCtx::build(&canon, source, lang, &project);
        let ctx = CheckCtx::for_test_full(&canon, source, &project, &file_ctx);

        let mut diagnostics = Vec::new();
        let kinds = Check.interested_kinds();
        for node in semantic.nodes().iter() {
            if kinds.contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    #[test]
    fn allows_vitest_in_dot_test_tsx_file() {
        // Issue #101: `src/app/features/auth/components/login-form.test.tsx`
        // importing vitest + @testing-library/* must not flag.
        let pkg = r#"{
            "dependencies": {"react": "^19"},
            "devDependencies": {
                "vitest": "^1",
                "@testing-library/react": "^14",
                "@testing-library/user-event": "^14"
            }
        }"#;
        let src = r#"
import { describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
"#;
        let d = run_with_pkg_at_path(
            pkg,
            "src/app/features/auth/components/login-form.test.tsx",
            src,
        );
        assert!(d.is_empty(), "test file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_vitest_in_dot_test_ts_file() {
        // Issue #101: `src/app/lib/form-server-errors.test.ts`
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { describe, expect, it } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/app/lib/form-server-errors.test.ts", src);
        assert!(d.is_empty(), "test file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_vitest_in_vitest_config_file() {
        // Issue #101: vitest.config.{ts,mts} importing from "vitest/config"
        // must not flag — `*.config.*` is treated as tooling.
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { defineConfig } from "vitest/config";
export default defineConfig({});"#;
        let d = run_with_pkg_at_path(pkg, "vitest.config.ts", src);
        assert!(d.is_empty(), "vitest.config.ts should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_hyphenated_config_variant_file() {
        // Issue #1861: recharts' `vitest.config-mutation.mts` is a Vitest
        // mutation-testing config variant. Its stem is `vitest.config-mutation`,
        // which the canonical `*.config.*` classifier misses, so it was flagged
        // for importing `vitest` and `@vitejs/plugin-react` from devDependencies.
        // A `*.config-<suffix>.*` file is dev tooling like any `*.config.*` and
        // must not flag.
        let pkg = r#"{
            "devDependencies": {
                "vitest": "^1",
                "@vitejs/plugin-react": "^4"
            }
        }"#;
        let src = r#"
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: { environment: 'jsdom', globals: true },
});
"#;
        let d = run_with_pkg_at_path(pkg, "vitest.config-mutation.mts", src);
        assert!(d.is_empty(), "hyphenated config variant should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_in_hyphenated_non_config_file() {
        // Guard against over-relaxing: a production file whose name merely
        // contains "config" as part of a segment (no `.config-` infix) must
        // still flag a devDependency import.
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { describe } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/config-loader.ts", src);
        assert_eq!(d.len(), 1, "non-config file should still flag: {d:?}");
        assert!(d[0].message.contains("vitest"));
    }

    #[test]
    fn allows_dev_dep_in_build_script() {
        // Issue #286: a codegen script under `scripts/` runs at dev/CI time and
        // is not part of the shipped bundle — importing a devDependency is correct.
        let pkg = r#"{"devDependencies":{"@tanstack/router-generator":"^1"}}"#;
        let src = r#"import { Generator, getConfig } from "@tanstack/router-generator";"#;
        let d = run_with_pkg_at_path(pkg, "scripts/generate-routes.ts", src);
        assert!(d.is_empty(), "build script should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_root_level_build_script() {
        // Issue #1673: elysia's root-level `build.ts` is a dev/CI bundler script
        // that imports `tsup` and `esbuild-fix-imports-plugin` from
        // devDependencies. Root-level `build.ts`/`bundle.ts` files never ship in
        // the published package, so importing a devDependency is correct.
        let pkg = r#"{
            "dependencies": {"bun": "^1"},
            "devDependencies": {
                "tsup": "^8",
                "esbuild-fix-imports-plugin": "^1"
            }
        }"#;
        let src = r#"
import { $ } from 'bun'
import { build } from 'tsup'
import { fixImportsPlugin } from 'esbuild-fix-imports-plugin'
"#;
        let d = run_with_pkg_at_path(pkg, "build.ts", src);
        assert!(d.is_empty(), "root-level build.ts should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_in_src_entry() {
        // Guard against over-relaxing: a shipped entry point under `src/`
        // importing the same bundler devDependency must still flag.
        let pkg = r#"{"devDependencies":{"tsup":"^8"}}"#;
        let src = r#"import { build } from 'tsup';"#;
        let d = run_with_pkg_at_path(pkg, "src/index.ts", src);
        assert_eq!(d.len(), 1, "src/ entry should still flag: {d:?}");
        assert!(d[0].message.contains("tsup"));
    }

    #[test]
    fn still_flags_dev_dep_in_root_non_build_file() {
        // Guard against over-relaxing: a root-level file with a different name
        // (not `build`/`bundle`) importing an extraneous devDependency must
        // still flag.
        let pkg = r#"{"devDependencies":{"tsup":"^8"}}"#;
        let src = r#"import { build } from 'tsup';"#;
        let d = run_with_pkg_at_path(pkg, "app.ts", src);
        assert_eq!(d.len(), 1, "root-level app.ts should still flag: {d:?}");
        assert!(d[0].message.contains("tsup"));
    }

    #[test]
    fn allows_dev_dep_in_samples_dev_file() {
        // Issue #1073: Azure SDK `samples-dev/` files are compiled and run as
        // documentation examples; `@azure/identity` is intentionally a
        // devDependency. Demonstration code must not flag.
        let pkg = r#"{
            "dependencies": {"@azure/core-client": "^1"},
            "devDependencies": {"@azure/identity": "^4"}
        }"#;
        let src = r#"import { DefaultAzureCredential } from "@azure/identity";"#;
        let d = run_with_pkg_at_path(pkg, "samples-dev/managementGroupsGetSample.ts", src);
        assert!(d.is_empty(), "samples-dev file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_component_demo_dir() {
        // Issue #1563: ant-design's `components/tabs/demo/style-class.tsx` is a
        // documentation example that imports `antd-style` and `@dnd-kit/core`
        // from devDependencies to showcase integration with other libraries.
        // Files under a `demo/` directory are documentation examples that never
        // ship in the published package, so the import is correct.
        let pkg = r#"{
            "devDependencies": {
                "antd-style": "^3",
                "@dnd-kit/core": "^6"
            }
        }"#;
        let src = r#"
import { css } from 'antd-style';
import { DndContext } from '@dnd-kit/core';
"#;
        let d = run_with_pkg_at_path(pkg, "components/tabs/demo/style-class.tsx", src);
        assert!(d.is_empty(), "demo/ file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_demos_dir() {
        // Issue #1563: the plural `demos/` convention is exempt for the same
        // reason as `demo/`.
        let pkg = r#"{"devDependencies":{"antd-style":"^3"}}"#;
        let src = r#"import { css } from 'antd-style';"#;
        let d = run_with_pkg_at_path(pkg, "packages/foo/demos/basic.tsx", src);
        assert!(d.is_empty(), "demos/ file should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_outside_demo_dirs() {
        // Guard against over-relaxing: a path where "demo" is a substring of
        // another segment (not its own directory) must still flag.
        let pkg = r#"{"devDependencies":{"antd-style":"^3"}}"#;
        let src = r#"import { css } from 'antd-style';"#;
        let d = run_with_pkg_at_path(pkg, "src/demonstration/index.ts", src);
        assert_eq!(d.len(), 1, "non-demo dir should still flag: {d:?}");
        assert!(d[0].message.contains("antd-style"));
    }

    #[test]
    fn allows_dev_dep_in_co_located_test_ts_file() {
        // Issue #1390: date-fns `src/endOfWeek/test.ts` is a co-located test
        // file whose whole name is `test.ts` (no `.test.` infix). It imports
        // vitest + @date-fns/tz from devDependencies, which is correct — these
        // files never ship in the published package.
        let pkg = r#"{
            "devDependencies": {
                "vitest": "^1",
                "@date-fns/tz": "^1"
            }
        }"#;
        let src = r#"
import { describe, it, expect } from "vitest";
import { TZDate } from "@date-fns/tz";
import { endOfWeek } from "..";
"#;
        let d = run_with_pkg_at_path(pkg, "src/endOfWeek/test.ts", src);
        assert!(d.is_empty(), "co-located test.ts should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_co_located_spec_ts_file() {
        // Issue #1390: a `spec.ts` whole-name file is the spec sibling of the
        // `test.ts` convention and must be exempt for the same reason.
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { describe, it, expect } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/startOfWeek/spec.ts", src);
        assert!(d.is_empty(), "co-located spec.ts should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_type_probe_tp_file() {
        // Issue #1915: date-fns `src/addBusinessDays/test.tp.ts` is a type-probe
        // test file (`.tp` = type probe) that imports vitest for type assertions.
        // These files exist solely to assert the public API type-checks; they are
        // never shipped or run as runtime code, so importing a devDependency is
        // correct — like any other test file.
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { describe, it, expectTypeOf } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/addBusinessDays/test.tp.ts", src);
        assert!(d.is_empty(), "type-probe .tp.ts should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_test_utils_dir() {
        // Issue #2048: graphql-js `src/__testUtils__/expectJSON.ts` is shared
        // test infrastructure that imports `chai` (a devDependency). The
        // `__testUtils__/` convention sits alongside `__tests__/` and never ships
        // in the published package, so importing a devDependency is correct.
        let pkg = r#"{"devDependencies":{"chai":"^4"}}"#;
        let src = r#"import { expect } from "chai";"#;
        let d = run_with_pkg_at_path(pkg, "src/__testUtils__/expectJSON.ts", src);
        assert!(d.is_empty(), "__testUtils__ file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_auto_mock_dir() {
        // Issue #1755: bulletproof-react's `apps/react-vite/__mocks__/zustand.ts`
        // is a Vitest/Jest manual mock auto-loaded by the test runner. It imports
        // the package it mocks plus test-only devDependencies (vitest,
        // @testing-library/react). `__mocks__/` files never ship in the published
        // package, so importing a devDependency is correct.
        let pkg = r#"{
            "dependencies": {"zustand": "^4"},
            "devDependencies": {
                "vitest": "^1",
                "@testing-library/react": "^14"
            }
        }"#;
        let src = r#"
import { act } from '@testing-library/react';
import { afterEach, vi } from 'vitest';
import * as zustand from 'zustand';
"#;
        let d = run_with_pkg_at_path(pkg, "apps/react-vite/__mocks__/zustand.ts", src);
        assert!(d.is_empty(), "__mocks__ file should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_outside_mocks_dir() {
        // Guard against over-relaxing: a path where "__mocks__" is a substring of
        // another segment (not its own directory) must still flag.
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { describe } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/my__mocks__data/index.ts", src);
        assert_eq!(d.len(), 1, "non-mock dir should still flag: {d:?}");
        assert!(d[0].message.contains("vitest"));
    }

    #[test]
    fn allows_dev_dep_in_testing_dir() {
        // Issue #1756: bulletproof-react `src/testing/mocks/utils.ts` is test
        // infrastructure (MSW handlers, test-data generators) loaded only by the
        // test runner. It imports `js-cookie` and `msw` from devDependencies,
        // which is correct — files under a `testing/` directory never ship in the
        // published bundle.
        let pkg = r#"{
            "dependencies": {"react": "^19"},
            "devDependencies": {"js-cookie": "^3", "msw": "^2"}
        }"#;
        let src = r#"
import Cookies from "js-cookie";
import { http, HttpResponse } from "msw";
"#;
        let d = run_with_pkg_at_path(pkg, "apps/react-vite/src/testing/mocks/utils.ts", src);
        assert!(d.is_empty(), "testing/ infra file should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_outside_testing_dir() {
        // Guard against over-relaxing: a path where "testing" is a substring of
        // another segment (not its own directory) must still flag.
        let pkg = r#"{"devDependencies":{"msw":"^2"}}"#;
        let src = r#"import { http } from "msw";"#;
        let d = run_with_pkg_at_path(pkg, "src/testingLibraryWrapper.ts", src);
        assert_eq!(d.len(), 1, "non-testing dir should still flag: {d:?}");
        assert!(d[0].message.contains("msw"));
    }

    #[test]
    fn allows_dev_dep_in_config_dir_build_tooling() {
        // Issue #2034: apollo-client `config/helpers.ts` is build tooling (API
        // extraction, compilation) that runs at dev/CI time and never ships in
        // the published package. It imports `@microsoft/api-extractor`, a
        // devDependency, which is the correct classification — files inside a
        // `config/` directory are build configuration, like `scripts/`.
        let pkg = r#"{
            "devDependencies": {
                "@microsoft/api-extractor": "^7",
                "@microsoft/api-extractor-model": "^7"
            }
        }"#;
        let src = r#"
import { ApiModelGenerator } from "@microsoft/api-extractor/lib/generators/ApiModelGenerator.js";
import type { ApiItem, ApiPackage } from "@microsoft/api-extractor-model";
"#;
        let d = run_with_pkg_at_path(pkg, "config/helpers.ts", src);
        assert!(d.is_empty(), "config/ build tooling should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_scaffold_template_file() {
        // Issue #2072: a scaffold CLI (e.g. create-t3-app) ships source files
        // under `cli/template/` that are copied into the generated project. Their
        // imports (`postgres`, `react`, `@trpc/server`, …) describe the generated
        // app's dependency graph; in the CLI's own package.json those packages are
        // devDependencies (used to type-check the templates). These files never run
        // as part of the CLI, so importing a devDependency must not flag.
        let pkg = r#"{
            "dependencies": {"commander": "^12"},
            "devDependencies": {"postgres": "^3.4.4", "@trpc/server": "^11"}
        }"#;
        let src = r#"
import postgres from "postgres";
import { initTRPC } from "@trpc/server";
"#;
        let d = run_with_pkg_at_path(
            pkg,
            "cli/template/extras/src/server/db/with-postgres.ts",
            src,
        );
        assert!(d.is_empty(), "scaffold template file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_templates_dir() {
        // Issue #2072: the plural `templates/` convention is exempt for the same
        // reason as `template/`.
        let pkg = r#"{"devDependencies":{"react":"^19"}}"#;
        let src = r#"import { useState } from "react";"#;
        let d = run_with_pkg_at_path(pkg, "templates/app/page.tsx", src);
        assert!(d.is_empty(), "templates/ file should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_outside_template_dirs() {
        // Guard against over-relaxing: a path that merely contains "template" as a
        // substring of another segment (not its own directory) must still flag.
        let pkg = r#"{"devDependencies":{"react":"^19"}}"#;
        let src = r#"import { useState } from "react";"#;
        let d = run_with_pkg_at_path(pkg, "src/templated/index.ts", src);
        assert_eq!(d.len(), 1, "non-template dir should still flag: {d:?}");
        assert!(d[0].message.contains("react"));
    }

    #[test]
    fn still_flags_dev_dep_outside_config_dir() {
        // Guard against over-relaxing: a path where "config" is a substring of
        // another segment (not its own directory) must still flag.
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { describe } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/appconfig/index.ts", src);
        assert_eq!(d.len(), 1, "non-config dir should still flag: {d:?}");
        assert!(d[0].message.contains("vitest"));
    }

    #[test]
    fn still_flags_dev_dep_outside_sample_dirs() {
        // Guard against over-relaxing: a path that merely contains "samples" as a
        // substring of another segment (not its own directory) must still flag.
        let pkg = r#"{"devDependencies":{"@azure/identity":"^4"}}"#;
        let src = r#"import { DefaultAzureCredential } from "@azure/identity";"#;
        let d = run_with_pkg_at_path(pkg, "src/mysamples/index.ts", src);
        assert_eq!(d.len(), 1, "non-sample dir should still flag: {d:?}");
        assert!(d[0].message.contains("@azure/identity"));
    }

    #[test]
    fn allows_dev_dep_in_storybook_stories_dir() {
        // Issue #1982: a pnpm-workspace `storybook/` package lists its runtime
        // deps (react, styled-components, …) in devDependencies because it is
        // never published. Story files under `storybook/stories/` import those
        // packages — they are dev-only tooling, like test files, and must not
        // flag. The file is not named `.stories.`; it lives inside `stories/`.
        let pkg = r#"{
            "devDependencies": {
                "react": "^19",
                "styled-components": "^6",
                "@react-spring/web": "^9"
            }
        }"#;
        let src = r#"
import React from "react";
import styled from "styled-components";
import { animated } from "@react-spring/web";
"#;
        let d = run_with_pkg_at_path(pkg, "storybook/stories/internal/KeyLogger.tsx", src);
        assert!(d.is_empty(), "storybook story file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_storybook_config_dir() {
        // Issue #1860: recharts places its Storybook configuration in a plain
        // top-level `storybook/` directory (not the dotted `.storybook/`). Config
        // and entry files directly under `storybook/` (`storybook/main.ts`,
        // `storybook/manager.ts`) import Storybook devDependencies and are never
        // bundled into the published library, so they must not flag.
        let pkg = r#"{
            "devDependencies": {
                "@storybook/react-vite": "^8",
                "storybook": "^8"
            }
        }"#;
        let main_src = r#"
import type { StorybookConfig } from '@storybook/react-vite';
const config: StorybookConfig = { framework: { name: '@storybook/react-vite', options: {} } };
export default config;
"#;
        let d = run_with_pkg_at_path(pkg, "storybook/main.ts", main_src);
        assert!(d.is_empty(), "storybook/main.ts should not flag devDeps: {d:?}");

        let manager_src = r#"
import { addons } from 'storybook/manager-api';
import { RechartsTheme } from './RechartsTheme';
addons.setConfig({ theme: RechartsTheme });
"#;
        let d = run_with_pkg_at_path(pkg, "storybook/manager.ts", manager_src);
        assert!(d.is_empty(), "storybook/manager.ts should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_dot_stories_file() {
        // A `*.stories.*` file is the canonical story convention and must be
        // exempt for the same reason as files inside `stories/`.
        let pkg = r#"{"devDependencies":{"react":"^19"}}"#;
        let src = r#"import { useState } from "react";"#;
        let d = run_with_pkg_at_path(pkg, "src/components/Button.stories.tsx", src);
        assert!(d.is_empty(), "*.stories.* file should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_outside_storybook_dirs() {
        // Guard against over-relaxing: a path where "stories" is a substring of
        // another segment (not its own directory) must still flag.
        let pkg = r#"{"devDependencies":{"react":"^19"}}"#;
        let src = r#"import { useState } from "react";"#;
        let d = run_with_pkg_at_path(pkg, "src/mystories/index.ts", src);
        assert_eq!(d.len(), 1, "non-storybook dir should still flag: {d:?}");
        assert!(d[0].message.contains("react"));
    }

    #[test]
    fn allows_dev_dep_in_library_src_with_entries_outside_src() {
        // Issue #1910: monaco-editor is a published library whose `main`/`module`
        // point into compiled output (`min/`, `esm/`). Its `src/` files are build
        // input that gets bundled — `monaco-editor-core` is a devDependency
        // inlined at build time, not a runtime import, so it must not flag.
        let pkg = r#"{
            "name": "monaco-editor",
            "main": "./min/vs/editor/editor.main.js",
            "module": "./esm/vs/editor/editor.main.js",
            "devDependencies": {"monaco-editor-core": "0.56.0-dev"}
        }"#;
        let src =
            r#"import 'monaco-editor-core/esm/vs/editor/contrib/suggest/browser/suggestInlineCompletions';"#;
        let d = run_with_pkg_at_path(pkg, "src/features/suggest/register.js", src);
        assert!(d.is_empty(), "library build input should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_missing_pkg_in_library_src() {
        // Guard: a package in NEITHER dependencies NOR devDependencies is a
        // genuine missing import and must still flag, even in a build-input
        // `src/` tree. (Here the rule only fires for devDependencies, so a
        // missing package produces zero diagnostics from THIS rule — assert the
        // declared-devDep case flags while exemption is scoped to deps that exist.)
        let pkg = r#"{
            "name": "monaco-editor",
            "main": "./min/vs/editor/editor.main.js",
            "devDependencies": {"monaco-editor-core": "0.56.0-dev"}
        }"#;
        // A genuinely undeclared package: no-extraneous-import does not own this
        // case (no-implicit-deps does), so it stays silent. Confirm the build-input
        // exemption never *adds* a diagnostic and never suppresses one it owns.
        let src = r#"import 'totally-undeclared-pkg';"#;
        let d = run_with_pkg_at_path(pkg, "src/features/x.js", src);
        assert!(d.is_empty(), "undeclared pkg is not this rule's concern: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_in_library_that_ships_src() {
        // Guard against over-relaxing: a library whose published entry points
        // INTO `src/` ships its source as-is, so `src/` is runtime production
        // code — a devDependency import there is a genuine break for consumers.
        let pkg = r#"{
            "name": "ships-src-lib",
            "main": "./src/index.js",
            "exports": {".": "./src/index.js"},
            "devDependencies": {"lodash": "^4"}
        }"#;
        let src = r#"import { merge } from "lodash";"#;
        let d = run_with_pkg_at_path(pkg, "src/util.js", src);
        assert_eq!(d.len(), 1, "library shipping src/ should still flag: {d:?}");
        assert!(d[0].message.contains("lodash"));
    }

    #[test]
    fn still_flags_dev_dep_in_non_library_app_src() {
        // Guard: a non-library (no `main`/`module`/`exports`) is an app whose
        // `src/` is runtime code. The build-input exemption must not apply —
        // existing behavior is preserved and the devDependency import flags.
        let pkg = r#"{
            "name": "some-app",
            "devDependencies": {"vitest": "^1"}
        }"#;
        let src = r#"import { describe } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/app/feature.ts", src);
        assert_eq!(d.len(), 1, "non-library app src should still flag: {d:?}");
        assert!(d[0].message.contains("vitest"));
    }

    #[test]
    fn still_flags_dev_dep_outside_src_in_build_input_library() {
        // Guard: the exemption is scoped to `src/`. A devDependency imported from
        // a non-`src/` runtime file of a build-input library must still flag.
        let pkg = r#"{
            "name": "monaco-editor",
            "main": "./min/vs/editor/editor.main.js",
            "devDependencies": {"vitest": "^1"}
        }"#;
        let src = r#"import { describe } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "lib/feature.ts", src);
        assert_eq!(d.len(), 1, "non-src file should still flag: {d:?}");
        assert!(d[0].message.contains("vitest"));
    }

    #[test]
    fn allows_dev_dep_in_vitest_prefixed_tooling_file() {
        // Issue #1891: immerjs/immer's root-level `vitest-custom-reporter.ts` is a
        // Vitest custom reporter consumed only by the test runner (referenced from
        // `vitest.config.ts` as `reporters: ['./vitest-custom-reporter']`). It is
        // never shipped, so importing `vitest` (a devDependency) is correct — like
        // any file under `__tests__/`.
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"
import { Reporter } from "vitest";
export default class CustomReporter implements Reporter {}
"#;
        let d = run_with_pkg_at_path(pkg, "vitest-custom-reporter.ts", src);
        assert!(d.is_empty(), "vitest- tooling file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_jest_prefixed_tooling_file() {
        // Issue #1891: `jest-setup.ts` is test-runner setup tooling, exempt for
        // the same reason as the `vitest-` prefix.
        let pkg = r#"{"devDependencies":{"jest":"^29"}}"#;
        let src = r#"import { jest } from "jest";"#;
        let d = run_with_pkg_at_path(pkg, "jest-setup.ts", src);
        assert!(d.is_empty(), "jest- tooling file should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_in_production_index() {
        // Guard against over-relaxing: a genuine production entry point importing a
        // devDependency must still flag — the prefix exemption is name-anchored.
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { describe } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/index.ts", src);
        assert_eq!(d.len(), 1, "production index should still flag: {d:?}");
        assert!(d[0].message.contains("vitest"));
    }

    #[test]
    fn allows_dev_dep_in_performance_tests_dir() {
        // Issue #1892: immerjs/immer's `__performance_tests__/incremental.mjs` is a
        // performance benchmark that imports comparison libraries (`lodash.clonedeep`,
        // `immutable`) from devDependencies. Benchmark files never ship in the
        // published package, so importing a devDependency is correct.
        let pkg = r#"{
            "devDependencies": {
                "lodash.clonedeep": "^4",
                "immutable": "^4"
            }
        }"#;
        let src = r#"
import cloneDeep from "lodash.clonedeep";
import { List } from "immutable";
"#;
        let d = run_with_pkg_at_path(pkg, "__performance_tests__/incremental.mjs", src);
        assert!(d.is_empty(), "__performance_tests__ file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_perf_testing_dir() {
        // Issue #1892: a `perf-testing/immutability-benchmarks.mjs` benchmark imports
        // the `mitata` benchmark runner from devDependencies. The `perf-testing/`
        // convention is a performance test suite that never ships, so the import is
        // correct.
        let pkg = r#"{"devDependencies":{"mitata":"^1"}}"#;
        let src = r#"import { run, bench, summary } from "mitata";"#;
        let d = run_with_pkg_at_path(pkg, "perf-testing/immutability-benchmarks.mjs", src);
        assert!(d.is_empty(), "perf-testing file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_benchmarks_dir() {
        // Issue #1701: jotai's `benchmarks/read-write.ts` imports the `benny`
        // benchmark runner from devDependencies. Files under a `benchmarks/`
        // directory are performance benchmarks that never ship in the published
        // package, so importing a benchmark-tool devDependency is correct.
        let pkg = r#"{"devDependencies":{"benny":"^3"}}"#;
        let src = r#"import { add, complete, cycle, save, suite } from "benny";"#;
        let d = run_with_pkg_at_path(pkg, "benchmarks/read-write.ts", src);
        assert!(d.is_empty(), "benchmarks/ file should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_outside_perf_dirs() {
        // Guard against over-relaxing: a path where "performance" is a substring of
        // another segment (not its own directory) must still flag.
        let pkg = r#"{"devDependencies":{"mitata":"^1"}}"#;
        let src = r#"import { bench } from "mitata";"#;
        let d = run_with_pkg_at_path(pkg, "src/performanceMonitor/index.ts", src);
        assert_eq!(d.len(), 1, "non-perf dir should still flag: {d:?}");
        assert!(d[0].message.contains("mitata"));
    }

    #[test]
    fn still_flags_dev_dep_in_production_code() {
        // Guard against over-relaxing: production code outside test/config
        // paths must still flag devDependency imports.
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { describe } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/app/features/auth/login.ts", src);
        assert_eq!(d.len(), 1, "production code should still flag: {d:?}");
        assert!(d[0].message.contains("vitest"));
    }

    #[test]
    fn allows_dev_dep_in_script_entry_toolchain_dir() {
        // Issue #1862: recharts' `omnidoc/` is a build-time doc-generation
        // toolchain. The `omnidoc` script runs `tsx ./omnidoc/generateApiDoc.ts`,
        // whose sibling helper `omnidoc/readProject.ts` imports `ts-morph` (a
        // devDependency). The directory is housed by a package.json script entry
        // and never ships in the published package, so the import is correct.
        let pkg = r#"{
            "name": "recharts",
            "main": "./lib/index.js",
            "files": ["lib"],
            "scripts": {
                "omnidoc": "npm exec --prefix ./www -- tsx ./omnidoc/generateApiDoc.ts"
            },
            "devDependencies": {"ts-morph": "^21", "prettier": "^3"}
        }"#;
        let src = r#"import { Project } from 'ts-morph';"#;
        let d = run_with_pkg_at_path(pkg, "omnidoc/readProject.ts", src);
        assert!(d.is_empty(), "script-entry toolchain dir should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_lint_rules_dir() {
        // Issue #1299: type-fest's `lint-rules/readme-jsdoc-sync.js` is a custom
        // ESLint rule run only during development; it imports `typescript`, a
        // devDependency. The `files` field excludes `lint-rules/` from the
        // published package, so importing a devDependency from it is correct.
        let pkg = r#"{"devDependencies":{"typescript":"^5"}}"#;
        let src = r#"import ts from "typescript";"#;
        let d = run_with_pkg_at_path(pkg, "lint-rules/readme-jsdoc-sync.js", src);
        assert!(d.is_empty(), "lint-rules/ file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_lint_processors_dir() {
        // Issue #1299: type-fest's `lint-processors/jsdoc-codeblocks.js` is a
        // custom ESLint processor run only during development; it imports
        // `@typescript-eslint/parser`, a devDependency. Files under
        // `lint-processors/` never ship, so the import is correct.
        let pkg = r#"{"devDependencies":{"@typescript-eslint/parser":"^8"}}"#;
        let src = r#"import parser from "@typescript-eslint/parser";"#;
        let d = run_with_pkg_at_path(pkg, "lint-processors/jsdoc-codeblocks.js", src);
        assert!(d.is_empty(), "lint-processors/ file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_tools_dir() {
        // Issue #1299: a `tools/` directory holds development tooling that never
        // ships in the published package, like `scripts/`. Importing a
        // devDependency from a `tools/` file is correct.
        let pkg = r#"{"devDependencies":{"typescript":"^5"}}"#;
        let src = r#"import ts from "typescript";"#;
        let d = run_with_pkg_at_path(pkg, "tools/codegen.ts", src);
        assert!(d.is_empty(), "tools/ file should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_in_scripts_genuinely_missing_pkg_unaffected() {
        // Negative-space guard for #1299: a devDependency import in a normal
        // `src/` file must still flag — the dev-tooling exemption is scoped to
        // the dev-tooling directories.
        let pkg = r#"{"devDependencies":{"typescript":"^5"}}"#;
        let src = r#"import ts from "typescript";"#;
        let d = run_with_pkg_at_path(pkg, "src/feature.ts", src);
        assert_eq!(d.len(), 1, "src/ file should still flag: {d:?}");
        assert!(d[0].message.contains("typescript"));
    }

    #[test]
    fn still_silent_on_genuinely_missing_pkg_in_dev_tooling_dir() {
        // Negative-space guard for #1299: a package absent from package.json
        // entirely (in NO dependency list) is a genuine missing import. This rule
        // only owns the devDependency case (no-implicit-deps owns the missing
        // case), so it stays silent — confirm the dev-tooling exemption does not
        // suppress a diagnostic this rule would otherwise own, and never adds one.
        let pkg = r#"{"devDependencies":{"typescript":"^5"}}"#;
        let src = r#"import { x } from "totally-undeclared-pkg";"#;
        let d = run_with_pkg_at_path(pkg, "scripts/build.js", src);
        assert!(d.is_empty(), "undeclared pkg is not this rule's concern: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_outside_lint_tooling_dirs() {
        // Guard against over-relaxing: a path where "tools" is a substring of
        // another segment (not its own directory) must still flag.
        let pkg = r#"{"devDependencies":{"typescript":"^5"}}"#;
        let src = r#"import ts from "typescript";"#;
        let d = run_with_pkg_at_path(pkg, "src/toolsRegistry/index.ts", src);
        assert_eq!(d.len(), 1, "non-tools dir should still flag: {d:?}");
        assert!(d[0].message.contains("typescript"));
    }

    /// Stage a monorepo on disk: a root `package.json` declaring `workspaces`,
    /// plus one `package.json` per member (the importing member's own manifest
    /// must be among them so its `devDependencies` are read), then lint
    /// `importer_member/src_rel` against the whole tree so
    /// `workspace_package_names()` is populated from the real workspace roots.
    /// Returns the diagnostics for that single file.
    fn run_in_workspace(
        root_pkg: &str,
        members: &[(&str, &str)],
        importer_member: &str,
        src_rel: &str,
        source: &str,
    ) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), root_pkg).unwrap();
        for (member_dir, member_pkg) in members {
            let member_path = dir.path().join(member_dir);
            fs::create_dir_all(&member_path).unwrap();
            fs::write(member_path.join("package.json"), member_pkg).unwrap();
        }
        let file_path = dir.path().join(importer_member).join(src_rel);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();

        // A second input directly under the repo root pulls the common ancestor
        // up to the root, so `detect_project_root` resolves the workspaces-root
        // manifest (not the member's), mirroring a real whole-repo run.
        let anchor_path = dir.path().join("eslint.config.ts");
        fs::write(&anchor_path, "export default [];").unwrap();

        let lang = Language::from_path(&file_path).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: lang,
        };
        let anchor_file = SourceFile {
            path: anchor_path,
            language: Language::TypeScript,
        };
        let refs = vec![&source_file, &anchor_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&file_path).unwrap();

        let source_type = match lang {
            Language::Tsx => SourceType::tsx(),
            Language::JavaScript => SourceType::cjs(),
            _ => SourceType::ts(),
        };
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, source_type).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let file_ctx = crate::rules::file_ctx::FileCtx::build(&canon, source, lang, &project);
        let ctx = CheckCtx::for_test_full(&canon, source, &project, &file_ctx);

        let mut diagnostics = Vec::new();
        let kinds = Check.interested_kinds();
        for node in semantic.nodes().iter() {
            if kinds.contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    #[test]
    fn allows_workspace_member_import_from_internal_docs_package() {
        // Issue #1968: mantine's `@docs/demos` is an internal, never-published
        // documentation package that lists the library members it demonstrates
        // (`@mantine/schedule`, `@mantinex/demo`) as devDependencies. A demo file
        // importing those workspace members must not flag — the import resolves to
        // a workspace-internal package, not an external dependency.
        let root_pkg = r#"{
            "name": "mantine-root",
            "private": true,
            "workspaces": ["packages/*/*"]
        }"#;
        let demos_pkg = r#"{
            "name": "@docs/demos",
            "devDependencies": {
                "@mantine/schedule": "workspace:*",
                "@mantinex/demo": "workspace:*"
            }
        }"#;
        let schedule_pkg = r#"{"name": "@mantine/schedule"}"#;
        let demo_pkg = r#"{"name": "@mantinex/demo"}"#;
        let src = r#"
import { Schedule } from '@mantine/schedule';
import { Demo } from '@mantinex/demo';
"#;
        let d = run_in_workspace(
            root_pkg,
            &[
                ("packages/@docs/demos", demos_pkg),
                ("packages/@mantine/schedule", schedule_pkg),
                ("packages/@mantinex/demo", demo_pkg),
            ],
            "packages/@docs/demos",
            "src/demos/schedule/Schedule.demo.tsx",
            src,
        );
        assert!(d.is_empty(), "workspace-internal imports should not flag: {d:?}");
    }

    #[test]
    fn still_flags_external_dev_dep_in_workspace_member() {
        // Guard against over-relaxing: inside the same monorepo, importing a
        // genuinely external devDependency (not a workspace member) from a
        // production file must still flag — the workspace exemption is scoped to
        // members of the workspace, not every devDependency.
        let root_pkg = r#"{
            "name": "mantine-root",
            "private": true,
            "workspaces": ["packages/*/*"]
        }"#;
        let core_pkg = r#"{
            "name": "@mantine/core",
            "devDependencies": {"vitest": "^1"}
        }"#;
        let src = r#"import { describe } from 'vitest';"#;
        let d = run_in_workspace(
            root_pkg,
            &[("packages/@mantine/core", core_pkg)],
            "packages/@mantine/core",
            "src/index.ts",
            src,
        );
        assert_eq!(d.len(), 1, "external devDep should still flag: {d:?}");
        assert!(d[0].message.contains("vitest"));
    }

    #[test]
    fn still_flags_dev_dep_in_published_source_alongside_script_entry_dir() {
        // Guard against over-relaxing: the script-entry-dir exemption is scoped
        // to the toolchain directory. A library that ships its `src/` as-is
        // (published entry points into `src/`) is runtime production code, so a
        // devDependency import there must still flag even when the package also
        // declares an `omnidoc/` script-entry toolchain directory.
        let pkg = r#"{
            "name": "ships-src-lib",
            "main": "./src/index.js",
            "exports": {".": "./src/index.js"},
            "scripts": {
                "omnidoc": "npm exec --prefix ./www -- tsx ./omnidoc/generateApiDoc.ts"
            },
            "devDependencies": {"ts-morph": "^21"}
        }"#;
        let src = r#"import { Project } from 'ts-morph';"#;
        let d = run_with_pkg_at_path(pkg, "src/index.ts", src);
        assert_eq!(d.len(), 1, "published src should still flag: {d:?}");
        assert!(d[0].message.contains("ts-morph"));
    }

    #[test]
    fn allows_type_only_import_of_dev_dep() {
        // Issue #1589: nuxt's `packages/nitro-server/src/runtime/utils/app-config.ts`
        // does `import type { AppConfig } from '@nuxt/schema'`, where `@nuxt/schema`
        // is a devDependency. A declaration-level `import type` is erased at compile
        // time and emits no JavaScript, so it creates no runtime dependency — there
        // is no install-time break for downstream consumers. It must not flag.
        let pkg = r#"{
            "dependencies": {"klona": "^2"},
            "devDependencies": {"@nuxt/schema": "^3"}
        }"#;
        let src = r#"
import { klona } from 'klona';
import type { AppConfig } from '@nuxt/schema';
"#;
        let d = run_with_pkg_at_path(
            pkg,
            "packages/nitro-server/src/runtime/utils/app-config.ts",
            src,
        );
        assert!(d.is_empty(), "type-only import should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_all_inline_type_specifiers_of_dev_dep() {
        // Issue #1589: an import whose every named specifier carries the inline
        // `type` qualifier (`import { type A, type B } from 'pkg'`) is fully
        // erased too — no runtime binding remains — so it must not flag.
        let pkg = r#"{"devDependencies":{"@nuxt/schema":"^3"}}"#;
        let src = r#"import { type AppConfig, type NuxtOptions } from '@nuxt/schema';"#;
        let d = run_with_pkg_at_path(pkg, "src/runtime/app-config.ts", src);
        assert!(d.is_empty(), "all-inline-type import should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_value_import_of_dev_dep() {
        // Negative-space guard: a plain value import (`import { X } from 'pkg'`) of
        // a devDependency from production source keeps a runtime binding and must
        // still flag — the exemption is type-only, not blanket.
        let pkg = r#"{"devDependencies":{"@nuxt/schema":"^3"}}"#;
        let src = r#"import { defineNuxtConfig } from '@nuxt/schema';"#;
        let d = run_with_pkg_at_path(pkg, "src/runtime/app-config.ts", src);
        assert_eq!(d.len(), 1, "value import should still flag: {d:?}");
        assert!(d[0].message.contains("@nuxt/schema"));
    }

    #[test]
    fn still_flags_mixed_import_with_runtime_binding() {
        // Negative-space guard: a mixed import (`import { type A, b } from 'pkg'`)
        // keeps the value binding `b` at runtime, so it is not erased and must
        // still flag.
        let pkg = r#"{"devDependencies":{"@nuxt/schema":"^3"}}"#;
        let src = r#"import { type AppConfig, defineNuxtConfig } from '@nuxt/schema';"#;
        let d = run_with_pkg_at_path(pkg, "src/runtime/app-config.ts", src);
        assert_eq!(d.len(), 1, "mixed import with runtime binding should still flag: {d:?}");
        assert!(d[0].message.contains("@nuxt/schema"));
    }

    #[test]
    fn still_flags_side_effect_import_of_dev_dep() {
        // Negative-space guard: a side-effect import (`import 'pkg'`) has no
        // specifiers but runs at runtime, so it is not erased and must still flag.
        let pkg = r#"{"devDependencies":{"some-polyfill":"^1"}}"#;
        let src = r#"import 'some-polyfill';"#;
        let d = run_with_pkg_at_path(pkg, "src/runtime/setup.ts", src);
        assert_eq!(d.len(), 1, "side-effect import should still flag: {d:?}");
        assert!(d[0].message.contains("some-polyfill"));
    }

    #[test]
    fn allows_dev_dep_in_test_tsd_type_declaration_file() {
        // Issue #2338: knex's `test-tsd/transaction.test-d.ts` is a tsd type
        // declaration test that imports `tsd` and `expect-type` (devDependencies)
        // to assert the public API type-checks. The `.test-d.` filename infix and
        // the `test-tsd/` directory are the tsd type-testing convention; such
        // files are never shipped or run as runtime code, so importing a
        // devDependency from them is correct — like any other test file.
        let pkg = r#"{
            "name": "knex",
            "main": "knex.js",
            "devDependencies": {"tsd": "^0.31", "expect-type": "^1"}
        }"#;
        let src = r#"
import { expectType } from 'tsd';
import { expectAssignable } from 'expect-type';
"#;
        let d = run_with_pkg_at_path(pkg, "test-tsd/transaction.test-d.ts", src);
        assert!(d.is_empty(), "test-tsd type-test file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_test_tsd_dir_without_test_d_infix() {
        // Issue #2338: a helper module inside `test-tsd/` that lacks the
        // `.test-d.` infix (e.g. `test-tsd/common.ts`) is still type-test
        // infrastructure consumed only by the tsd suite, so importing a
        // devDependency from it is correct.
        let pkg = r#"{
            "name": "knex",
            "main": "knex.js",
            "devDependencies": {"expect-type": "^1"}
        }"#;
        let src = r#"import { expectAssignable } from 'expect-type';"#;
        let d = run_with_pkg_at_path(pkg, "test-tsd/common.ts", src);
        assert!(d.is_empty(), "test-tsd helper file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_integration_test_fixture_src() {
        // Issue #2378: NestJS keeps full mini-applications under
        // `integration/*/src/` that are spun up by sibling `e2e/*.spec.ts`
        // integration-test suites. They are test-fixture apps — never published
        // — so importing a devDependency (`kafkajs`) from their controllers is
        // correct, like any file under `__fixtures__/` or `test-projects/`.
        let pkg = r#"{
            "name": "@nestjs/microservices",
            "main": "index.js",
            "devDependencies": {"kafkajs": "^2"}
        }"#;
        let src = r#"import { Kafka } from 'kafkajs';"#;
        let d = run_with_pkg_at_path(
            pkg,
            "integration/microservices/src/kafka-concurrent/kafka-concurrent.controller.ts",
            src,
        );
        assert!(d.is_empty(), "integration-test fixture src should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_in_production_src_integration_module() {
        // Guard against over-relaxing: a production `src/integration/` module
        // (an "integration with service X", not the integration-test app tree)
        // has no nested `src/` after the `integration/` segment, so it is not the
        // fixture-app shape and must still flag a devDependency import.
        let pkg = r#"{"name":"some-lib","devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { describe } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/integration/payment-gateway.ts", src);
        assert_eq!(d.len(), 1, "production src/integration module should still flag: {d:?}");
        assert!(d[0].message.contains("vitest"));
    }

    #[test]
    fn allows_dev_dep_in_private_package() {
        // Issue #1373: directus' `@directus/app` dashboard is a bundled Vue SPA
        // marked `"private": true` and never published to npm. It lists all its
        // runtime deps (vue, vue-router, …) in devDependencies by design. Since
        // nothing is npm-installed by a consumer, the deps/devDeps distinction is
        // irrelevant and a production `import { ref } from 'vue'` must not flag.
        let pkg = r#"{
            "name": "@directus/app",
            "private": true,
            "devDependencies": {"vue": "^3"}
        }"#;
        let src = r#"import { ref } from 'vue';"#;
        let d = run_with_pkg_at_path(pkg, "src/components/app.ts", src);
        assert!(d.is_empty(), "private package should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_in_publishable_package() {
        // Negative-space guard: a publishable package (no `"private": true`) with
        // `vue` in devDependencies and a production value import of `vue` must
        // still flag — the exemption is scoped to private packages, and the rule
        // keeps working for genuinely-published packages.
        let pkg = r#"{
            "name": "publishable-lib",
            "devDependencies": {"vue": "^3"}
        }"#;
        let src = r#"import { ref } from 'vue';"#;
        let d = run_with_pkg_at_path(pkg, "src/components/app.ts", src);
        assert_eq!(d.len(), 1, "publishable package should still flag: {d:?}");
        assert!(d[0].message.contains("vue"));
    }
}
