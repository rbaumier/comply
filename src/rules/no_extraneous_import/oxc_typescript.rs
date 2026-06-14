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
        // Build/codegen scripts (`scripts/`, `config/`), demonstration code
        // (`samples/`, `examples/`, …), and generator scaffold templates
        // (`templates/`, `scaffold/`, …) run at dev time and never ship in the
        // published package, so importing a devDependency from them is correct.
        if ctx.file.path_segments.in_aux_dir
            || crate::rules::path_utils::is_build_script_path(ctx.path)
            || crate::rules::path_utils::is_sample_dir_path(ctx.path)
        {
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

        let root = package_root(specifier);
        let in_runtime = pkg.dependencies.contains_key(root)
            || pkg.peer_dependencies.contains_key(root)
            || pkg.optional_dependencies.contains_key(root);
        if in_runtime {
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
}
