//! node-no-top-level-await OXC backend.
//!
//! Flags top-level `await` in published CommonJS modules, where it is invalid.
//! Exempt are files in an ES-module context (a `.mjs`/`.mts` extension, or a
//! nearest `package.json` declaring `"type": "module"`), where top-level await
//! is a valid Stage-4 feature, plus test files, `__mocks__/` manual-mock files
//! (Jest/Vitest test infrastructure that uses top-level `await` for
//! `vi.importActual()`), standalone scripts run directly (files under a
//! `scripts/`, `bench/`, `benchmarks/`, or `benchmark/` directory, or carrying a
//! `#!` shebang), package-root build/CLI scripts invoked directly by a
//! `package.json` `scripts` entry (e.g. `tsx ./build.ts`), and entrypoints.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if TEST_MARKERS.iter().any(|m| s.contains(m)) {
        return true;
    }
    path.components()
        .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "e2e")
}

fn is_script_file(path: &std::path::Path, source: &str) -> bool {
    const SCRIPT_DIRS: &[&str] = &["scripts", "bench", "benchmarks", "benchmark"];
    if path
        .components()
        .any(|c| c.as_os_str().to_str().is_some_and(|seg| SCRIPT_DIRS.contains(&seg)))
    {
        return true;
    }
    source.starts_with("#!")
}

fn is_entrypoint(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, ".listen(") || crate::oxc_helpers::source_contains(source, "process.exit")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["await"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };

        if crate::rules::module_system::is_es_module_context_cached(ctx)
            || is_test_file(ctx.path)
            || crate::rules::path_utils::is_auto_mock_dir_path(ctx.path)
            || is_script_file(ctx.path, ctx.source)
            || ctx.project.is_script_entry_file(ctx.path)
            || is_entrypoint(ctx.source)
        {
            return;
        }

        // Walk up: if inside any function scope, this is not top-level.
        for ancestor in semantic.nodes().ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_) => {
                    return; // Inside a function — not top-level.
                }
                _ => {}
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Top-level `await` is forbidden in published modules.".into(),
            severity: Severity::Error,
            span: None,
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

    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
    }

    #[test]
    fn flags_top_level_await() {
        let d = run_at("const data = await fetch('/api');", "src/load.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Top-level"));
    }

    #[test]
    fn allows_await_in_async_function() {
        let src = "async function load() { const data = await fetch('/api'); }";
        assert!(run_at(src, "src/load.ts").is_empty());
    }

    // Regression for #1757: manual-mock files in `__mocks__/` are Vitest/Jest
    // test infrastructure that legitimately use top-level `await` (e.g.
    // `vi.importActual()`); they never ship in the published package.
    #[test]
    fn allows_top_level_await_in_auto_mock_dir() {
        let src = r#"
import { act } from '@testing-library/react';
import { afterEach, vi } from 'vitest';
import * as zustand from 'zustand';

const { create: actualCreate, createStore: actualCreateStore } =
  await vi.importActual<typeof zustand>('zustand');
"#;
        assert!(run_at(src, "apps/react-vite/__mocks__/zustand.ts").is_empty());
    }

    // Regression for #1664: benchmark runners under a `bench/` directory are
    // standalone scripts executed directly with `node`, never imported as
    // published modules; top-level `await` sequences their async steps. Exempt
    // like `scripts/`. The `bench` segment may be nested (here under `docs/`).
    #[test]
    fn allows_top_level_await_in_bench_dir() {
        let src = r#"
console.log(`Crawl throughput benchmark (${PAGE_COUNT} pages, ${TRIALS} trials)`)
await benchmark('Current implementation')
"#;
        assert!(run_at(src, "docs/src/prerender/bench/src/crawl-throughput.ts").is_empty());
    }

    // Regression for #1664: the same exemption for a JavaScript benchmark
    // runner under `bench/` (esbuild-style build/watch driver).
    #[test]
    fn allows_top_level_await_in_bench_dir_js() {
        let src = r#"
let ctx = await context(buildOptions)
await ctx.watch()
"#;
        assert!(run_at(src, "packages/ui/bench/frameworks/solid/build.js").is_empty());
    }

    // Negative space for #1664: a normal library module under `src/` (no
    // `bench`/`benchmark` segment) must still be flagged — the exemption is
    // keyed on the directory convention, not relaxed for every file.
    #[test]
    fn still_flags_top_level_await_outside_bench_dir() {
        let d = run_at("const data = await fetch('/api');", "src/benchmarking/run.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Top-level"));
    }

    /// Run the rule against `importer_rel` inside a temp tree whose root
    /// `package.json` is `pkg_json`, exercising the on-disk
    /// `nearest_package_json` ESM detection.
    fn run_in_package(importer_rel: &str, source: &str, pkg_json: &str) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let importer = dir.path().join(importer_rel);
        std::fs::create_dir_all(importer.parent().unwrap()).unwrap();
        std::fs::write(&importer, source).unwrap();
        let canon = std::fs::canonicalize(&importer).unwrap();
        let source_file = SourceFile {
            path: canon.clone(),
            language: Language::from_path(&canon).unwrap(),
        };
        let project = ProjectCtx::load(&[&source_file], &Config::default());
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    // Regression for #1776: top-level `await` is valid in an ES module, so a
    // file under a `package.json` that declares `"type": "module"` (sst's
    // `sdk/js/` package) must not be flagged.
    #[test]
    fn allows_top_level_await_in_type_module_package() {
        let src = r#"
import { Issuer } from "openid-client";
import { OauthAdapter, OauthBasicConfig } from "./oauth.js";

const issuer = await Issuer.discover(
  "https://appleid.apple.com/.well-known/openid-configuration",
);

export const AppleAdapter = (config: OauthBasicConfig) => {
  return OauthAdapter({ issuer });
};
"#;
        let pkg = r#"{ "type": "module", "exports": { ".": "./dist/index.js" } }"#;
        assert!(
            run_in_package("src/auth/adapter/apple.ts", src, pkg).is_empty(),
            "top-level await in a \"type\":\"module\" package is valid ESM"
        );
    }

    // A CommonJS package (no `"type": "module"`) still forbids top-level await.
    #[test]
    fn flags_top_level_await_in_commonjs_package() {
        let src = "const data = await fetch('/api');";
        let pkg = r#"{ "main": "./dist/index.js" }"#;
        let diags = run_in_package("src/load.ts", src, pkg);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Top-level"));
    }

    // Regression for #1690: a package-root `build.ts` invoked directly by the
    // `scripts.build` entry (`tsx ./build.ts`) is a one-shot executable script,
    // not a published module, even when the package itself is CommonJS and
    // declares `main`/`exports`. Top-level await is idiomatic there.
    #[test]
    fn allows_top_level_await_in_package_root_build_script() {
        let src = r#"
import { buildCjs, buildEsm } from '@redwoodjs/framework-tools'

await buildEsm()
await buildCjs()
"#;
        let pkg = r#"{
  "scripts": { "build": "tsx ./build.ts" },
  "main": "./dist/cjs/index.js",
  "exports": { ".": { "import": "./dist/esm/index.js" } }
}"#;
        assert!(
            run_in_package("build.ts", src, pkg).is_empty(),
            "top-level await in a package-root build.ts run via tsx is not a published module"
        );
    }

    // Negative space for #1690: an ordinary library module under `src/` in the
    // same CommonJS package — one that is NOT a script entry — must still be
    // flagged. The script-entry exemption is keyed on the `scripts` invocation,
    // not on the mere presence of a `scripts` field.
    #[test]
    fn still_flags_top_level_await_in_library_module_of_build_script_package() {
        let src = "const data = await fetch('/api');";
        let pkg = r#"{
  "scripts": { "build": "tsx ./build.ts" },
  "main": "./dist/cjs/index.js"
}"#;
        let diags = run_in_package("src/load.ts", src, pkg);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Top-level"));
    }
}
