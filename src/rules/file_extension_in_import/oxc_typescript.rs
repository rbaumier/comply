//! file-extension-in-import OXC backend — flag relative imports missing a file extension.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const KNOWN_EXTENSIONS: &[&str] = &[
    ".js", ".ts", ".tsx", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json", ".css", ".scss",
    ".less", ".svg", ".png", ".vue", ".svelte",
];

const BUNDLER_DEPS: &[&str] = &[
    "vite",
    "webpack",
    "next",
    "esbuild",
    "parcel",
    "rollup",
    "@parcel/core",
    "@rspack/core",
    "rspack",
    "turbopack",
    "metro",
    "bun",
    "@swc/core",
    "tsup",
];

const BUNDLER_CONFIG_FILES: &[&str] = &[
    "vite.config.ts",
    "vite.config.js",
    "vite.config.mts",
    "vite.config.mjs",
    "vite.config.cts",
    "vite.config.cjs",
    "vitest.config.ts",
    "vitest.config.js",
    "vitest.config.mts",
    "vitest.config.mjs",
    "vitest.config.cts",
    "vitest.config.cjs",
    "webpack.config.ts",
    "webpack.config.js",
    "webpack.config.mts",
    "webpack.config.mjs",
    "webpack.config.cts",
    "webpack.config.cjs",
    "next.config.ts",
    "next.config.js",
    "next.config.mjs",
    "next.config.cjs",
    "turbopack.config.ts",
    "turbopack.config.js",
];

fn has_known_extension(spec: &str) -> bool {
    // Strip a bundler resource query (`./App.vue?raw`, `./img.png?url`,
    // `./styles.css?inline`) before matching: the `?suffix` instructs Vite/
    // webpack how to transform the module and does not change the underlying
    // path, so the extension is still present and cannot be dropped.
    let base = spec.split_once('?').map_or(spec, |(base, _)| base);
    KNOWN_EXTENSIONS.iter().any(|ext| base.ends_with(ext))
}

fn is_directory_import(spec: &str) -> bool {
    spec.ends_with('/')
        || spec.ends_with("/index")
        // A specifier whose final segment is `.` or `..` (e.g. `../../../..`,
        // `..`, `./sub/.`) navigates to a directory and resolves via that
        // directory's `index.js` / package.json `main` — it is not a file
        // missing an extension.
        || spec == ".."
        || spec == "."
        || spec.ends_with("/..")
        || spec.ends_with("/.")
}

fn is_relative(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../")
}

pub(crate) fn project_uses_bundler(ctx: &CheckCtx) -> bool {
    if let Some(pkg) = ctx.project.nearest_package_json(ctx.path)
        && (BUNDLER_DEPS.iter().any(|dep| pkg.has_dep_or_engine(dep))
            || pkg.all_deps().any(|dep| dep.starts_with("@vitejs/")))
    {
        return true;
    }
    has_bundler_config(ctx.path)
}

fn has_bundler_config(path: &std::path::Path) -> bool {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if BUNDLER_CONFIG_FILES
            .iter()
            .any(|name| d.join(name).is_file())
        {
            return true;
        }
        dir = d.parent();
    }
    false
}

fn tsconfig_uses_bundler_resolution(ctx: &CheckCtx) -> bool {
    let Some(ts) = ctx.project.nearest_tsconfig(ctx.path) else {
        return false;
    };
    ts.module_resolution
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case("bundler"))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[] // full-program analysis
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Every probe is directory-invariant (manifest + bundler config +
        // tsconfig chain), so memoize the combined skip decision per directory:
        // `has_bundler_config` stat-walks the ancestor tree, which is otherwise
        // re-run for every file in a deep monorepo.
        let skip_for_bundler = ctx.project.cached_bundler(ctx.path, || {
            project_uses_bundler(ctx)
                || tsconfig_uses_bundler_resolution(ctx)
                || ctx.project.is_commonjs_project(ctx.path)
        });
        if skip_for_bundler {
            return Vec::new();
        }

        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for node in nodes.iter() {
            match node.kind() {
                AstKind::ImportDeclaration(import) => {
                    let spec = import.source.value.as_str();
                    check_specifier(spec, import.source.span.start, ctx, &mut diagnostics);
                }
                AstKind::ExportNamedDeclaration(export) => {
                    if let Some(source) = &export.source {
                        let spec = source.value.as_str();
                        check_specifier(spec, source.span.start, ctx, &mut diagnostics);
                    }
                }
                AstKind::ExportAllDeclaration(export) => {
                    let spec = export.source.value.as_str();
                    check_specifier(spec, export.source.span.start, ctx, &mut diagnostics);
                }
                _ => {}
            }
        }

        diagnostics
    }
}

fn check_specifier(
    spec: &str,
    span_start: u32,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_relative(spec) {
        return;
    }
    if has_known_extension(spec) {
        return;
    }
    if is_directory_import(spec) {
        return;
    }
    // A relative import resolving into a Prisma `generator { output = … }`
    // directory (a custom output dir such as `./client`) targets the generated
    // client. Prisma emits ESM with explicit extensions internally; the user's
    // import of the generated entry is resolved by the generated package's own
    // exports map, so requiring an extension here is a false positive.
    if crate::rules::path_utils::resolves_into_prisma_output(ctx.path, spec, ctx.project) {
        return;
    }
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Relative import `{spec}` is missing a file extension. Add an explicit extension (e.g. `.js`, `.ts`) for ESM compatibility.",
        ),
        severity: Severity::Error,
        span: None,
    });
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
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Build a project rooted at a tempdir with the given root-level files, then
    /// run the OXC backend against `src/app.ts`. Returns the diagnostics so a
    /// test can assert whether the extensionless import was flagged.
    fn run_in_project(root_files: &[(&str, &str)], source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        for (name, contents) in root_files {
            fs::write(dir.path().join(name), contents).unwrap();
        }
        let file_path = dir.path().join("src/app.ts");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::from_path(&file_path).unwrap(),
        };
        let refs = vec![&source_file];
        let project = ProjectCtx::load(&refs, &Config::default());
        let canon = fs::canonicalize(&file_path).unwrap();
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    /// Build a project rooted at a tempdir with the given files (relative paths
    /// from the root), then run the OXC backend against `target_rel`. Lets a test
    /// stage a subtree (e.g. `tests/`) with its own tsconfig to exercise
    /// nearest-config precedence.
    fn run_with_files(
        files: &[(&str, &str)],
        target_rel: &str,
        source: &str,
    ) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        for (rel, contents) in files {
            let p = dir.path().join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, contents).unwrap();
        }
        let file_path = dir.path().join(target_rel);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::from_path(&file_path).unwrap(),
        };
        let refs = vec![&source_file];
        let project = ProjectCtx::load(&refs, &Config::default());
        let canon = fs::canonicalize(&file_path).unwrap();
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    // Regression for #1307: a `tests/` subtree with its own tsconfig selecting
    // `moduleResolution:node` (classic resolution accepts extensionless imports)
    // is governed by that closer tsconfig, even though the root package.json
    // declares `"type":"module"`. The rule must stay silent there.
    #[test]
    fn skips_tests_subtree_with_classic_resolution_tsconfig_issue_1307() {
        let diags = run_with_files(
            &[
                ("package.json", r#"{"type":"module"}"#),
                (
                    "tsconfig.json",
                    r#"{"compilerOptions":{"moduleResolution":"bundler"},"exclude":["tests/"]}"#,
                ),
                (
                    "tests/tsconfig.json",
                    r#"{"compilerOptions":{"moduleResolution":"node"}}"#,
                ),
            ],
            "tests/foo.test.ts",
            "import { x } from './util';\n",
        );
        assert!(
            diags.is_empty(),
            "tests/ tsconfig with moduleResolution:node governs over root type:module: {diags:?}"
        );
    }

    // Negative space for #1307 (a): the same extensionless import in a root-level
    // ESM file — nearest tsconfig is ESM (`moduleResolution:bundler` excludes
    // tests/, so root files have no classic signal) and package.json is
    // type:module — must still be flagged.
    #[test]
    fn still_flags_root_esm_file_with_type_module_issue_1307() {
        let diags = run_with_files(
            &[
                ("package.json", r#"{"type":"module"}"#),
                (
                    "tsconfig.json",
                    r#"{"compilerOptions":{"moduleResolution":"nodenext"}}"#,
                ),
            ],
            "src/app.ts",
            "import { x } from './util';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "root ESM file (type:module, nodenext) still requires extensions: {diags:?}"
        );
    }

    // Negative space for #1307 (b): a `tests/` subtree whose own tsconfig selects
    // `nodenext` (dual-mode, ESM-capable) is NOT a classic signal, so even with
    // its closer tsconfig the rule keeps firing.
    #[test]
    fn still_flags_tests_subtree_with_nodenext_tsconfig_issue_1307() {
        let diags = run_with_files(
            &[
                ("package.json", r#"{"type":"module"}"#),
                (
                    "tsconfig.json",
                    r#"{"compilerOptions":{"moduleResolution":"bundler"},"exclude":["tests/"]}"#,
                ),
                (
                    "tests/tsconfig.json",
                    r#"{"compilerOptions":{"moduleResolution":"nodenext"}}"#,
                ),
            ],
            "tests/foo.test.ts",
            "import { x } from './util';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "tests/ tsconfig with nodenext is ESM-capable — still flag: {diags:?}"
        );
    }

    // Regression for #7606: the nearest tsconfig `extends` a workspace/published
    // package by its package name (`@scope/base/tsconfig.json`), and the
    // inherited `moduleResolution:bundler` lives only in that extended base.
    // Node-module resolution of the package `extends` surfaces the inherited
    // option, so extensionless relative imports (Backstage's house style) stay
    // silent.
    #[test]
    fn skips_when_bundler_resolution_inherited_via_package_extends_issue_7606() {
        let diags = run_with_files(
            &[
                ("package.json", r#"{"type":"module"}"#),
                ("tsconfig.json", r#"{"extends":"@scope/base/tsconfig.json"}"#),
                (
                    "node_modules/@scope/base/package.json",
                    r#"{"name":"@scope/base"}"#,
                ),
                (
                    "node_modules/@scope/base/tsconfig.json",
                    r#"{"compilerOptions":{"moduleResolution":"bundler"}}"#,
                ),
            ],
            "src/app.ts",
            "import { x } from './util';\n",
        );
        assert!(
            diags.is_empty(),
            "moduleResolution:bundler inherited via package `extends` must silence the rule: {diags:?}"
        );
    }

    // Negative space for #7606: the same package-`extends` shape whose base sets
    // `moduleResolution:nodenext` (ESM, not bundler) still flags the
    // extensionless import — the real inherited value is read, not blanket-
    // suppressed by the presence of a package `extends`.
    #[test]
    fn still_flags_when_nodenext_inherited_via_package_extends_issue_7606() {
        let diags = run_with_files(
            &[
                ("package.json", r#"{"type":"module"}"#),
                ("tsconfig.json", r#"{"extends":"@scope/base/tsconfig.json"}"#),
                (
                    "node_modules/@scope/base/package.json",
                    r#"{"name":"@scope/base"}"#,
                ),
                (
                    "node_modules/@scope/base/tsconfig.json",
                    r#"{"compilerOptions":{"moduleResolution":"nodenext"}}"#,
                ),
            ],
            "src/app.ts",
            "import { x } from './util';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "nodenext inherited via package `extends` is not bundler — still flag: {diags:?}"
        );
    }

    // Regression for #1712: Angular's build toolchain resolves extensionless
    // relative imports, so the rule must stay silent when Angular is detected.

    // Regression for #2243: a monorepo whose root carries vitest.config.ts (a
    // Vite-based bundler) but no vite.config.ts must be recognised as a bundler
    // project, so extensionless relative imports in sub-packages stay silent.
    #[test]
    fn skips_when_root_vitest_config_present() {
        let diags = run_in_project(
            &[("vitest.config.ts", "export default {}")],
            "export { createTestingPinia } from './testing';\n",
        );
        assert!(
            diags.is_empty(),
            "vitest.config.ts → bundler context → silence: {diags:?}"
        );
    }

    // Regression for #2340: a relative specifier whose final segment is `..`
    // (e.g. `../../../..`, `../..`, `..`) navigates to a directory and resolves
    // via that directory's `index.js` / package.json `main`, so it is a
    // directory import — not a file missing an extension.
    #[test]
    fn skips_trailing_dotdot_directory_navigation() {
        let pkg = r#"{"type":"module","dependencies":{}}"#;
        let diags = run_in_project(
            &[("package.json", pkg)],
            "import { Knex } from '../../../..';\nimport x from '..';\nimport y from '../..';\n",
        );
        assert!(
            diags.is_empty(),
            "trailing `..` specifiers are directory imports, not files: {diags:?}"
        );
    }

    // Regression for #2117: a CommonJS-configured sub-package (no `type:module`
    // in package.json + `module:"commonjs"` in tsconfig) resolves relative
    // imports via CJS require(), which never requires file extensions. The rule
    // must stay silent there.
    #[test]
    fn skips_commonjs_configured_project_issue_2117() {
        let pkg = r#"{"name":"@fluentui/workspace-plugin"}"#;
        let tsconfig = r#"{"compilerOptions":{"module":"commonjs"}}"#;
        let diags = run_in_project(
            &[("package.json", pkg), ("tsconfig.json", tsconfig)],
            "import { type PackageJson } from '../types';\n",
        );
        assert!(
            diags.is_empty(),
            "CommonJS project (no type:module + module:commonjs) must not require extensions: {diags:?}"
        );
    }

    // Negative space for #2117: a package that IS ESM (`type:module`) but whose
    // tsconfig still says `module:commonjs` declares ESM intent, so extensions
    // remain required — the CJS exemption keys on the ABSENCE of `type:module`.
    #[test]
    fn still_flags_type_module_even_with_commonjs_tsconfig() {
        let pkg = r#"{"name":"pkg","type":"module"}"#;
        let tsconfig = r#"{"compilerOptions":{"module":"commonjs"}}"#;
        let diags = run_in_project(
            &[("package.json", pkg), ("tsconfig.json", tsconfig)],
            "import { columns } from './makeData';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "type:module declares ESM intent, extensions still required: {diags:?}"
        );
    }

    // Regression for #7558: a CommonJS package that opts into `node16`/`nodenext`
    // (for `exports`-map support) while emitting CommonJS — no `"type":"module"`
    // in package.json — resolves relative imports via CJS require(), which never
    // requires extensions. TypeScript reads each file's format from the nearest
    // package.json `type`, so the rule must stay silent. Covers both `module` and
    // `moduleResolution` spellings, case-insensitively.
    #[test]
    fn skips_node16_module_without_type_field_issue_7558() {
        let pkg = r#"{"name":"@medusajs/utils","main":"dist/index.js"}"#;
        let tsconfig = r#"{"compilerOptions":{"module":"Node16","moduleResolution":"Node16"}}"#;
        let diags = run_in_project(
            &[("package.json", pkg), ("tsconfig.json", tsconfig)],
            "export * from './order-change';\n",
        );
        assert!(
            diags.is_empty(),
            "CJS package on module:Node16 (no type:module) resolves via require(): {diags:?}"
        );
    }

    #[test]
    fn skips_nodenext_module_resolution_without_type_field_issue_7558() {
        let pkg = r#"{"name":"pkg","main":"dist/index.js"}"#;
        let tsconfig = r#"{"compilerOptions":{"moduleResolution":"NodeNext"}}"#;
        let diags = run_in_project(
            &[("package.json", pkg), ("tsconfig.json", tsconfig)],
            "import { columns } from './makeData';\n",
        );
        assert!(
            diags.is_empty(),
            "CJS package on moduleResolution:NodeNext (no type:module) is CommonJS: {diags:?}"
        );
    }

    // Negative space for #7558: the SAME `node16` tsconfig but with the nearest
    // package.json declaring `"type":"module"` makes each file ESM, where
    // extensions are required — so the rule must keep firing. Guards the #1307
    // ESM path from over-suppression.
    #[test]
    fn still_flags_node16_module_with_type_module_issue_7558() {
        let pkg = r#"{"name":"pkg","type":"module"}"#;
        let tsconfig = r#"{"compilerOptions":{"module":"Node16","moduleResolution":"Node16"}}"#;
        let diags = run_in_project(
            &[("package.json", pkg), ("tsconfig.json", tsconfig)],
            "import { columns } from './makeData';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "type:module on node16 is ESM — extensions still required: {diags:?}"
        );
    }

    // Regression for #7781: a CommonJS package whose tsconfig delegates
    // `module`/`moduleResolution` via `extends` into a rig config inside
    // node_modules (absent without `npm install`) is left with a tsconfig silent
    // on module format. Node's default for a package.json without `"type"` is
    // CommonJS, whose require() resolver supplies extensions — so extensionless
    // relative imports are correct and the rule must stay silent.
    #[test]
    fn skips_commonjs_package_with_module_inherited_via_unresolvable_extends_issue_7781() {
        let pkg = r#"{"name":"@hcengineering/core","main":"lib/index.js"}"#;
        let tsconfig = r#"{"extends":"./node_modules/@scope/rig/tsconfig.json","compilerOptions":{"rootDir":"src","outDir":"lib"}}"#;
        let diags = run_in_project(
            &[("package.json", pkg), ("tsconfig.json", tsconfig)],
            "import { Status, StatusValue } from './classes';\n",
        );
        assert!(
            diags.is_empty(),
            "CommonJS package (no type field) with module config inherited via unresolvable extends resolves via require(): {diags:?}"
        );
    }

    // Negative space for #7781: an explicit `"type":"commonjs"` with a tsconfig
    // silent on module format is likewise CommonJS — extensionless imports stay
    // silent.
    #[test]
    fn skips_explicit_commonjs_type_with_silent_tsconfig_issue_7781() {
        let pkg = r#"{"name":"pkg","type":"commonjs","main":"lib/index.js"}"#;
        let tsconfig = r#"{"compilerOptions":{"rootDir":"src","outDir":"lib"}}"#;
        let diags = run_in_project(
            &[("package.json", pkg), ("tsconfig.json", tsconfig)],
            "import { x } from './util';\n",
        );
        assert!(
            diags.is_empty(),
            "explicit type:commonjs with silent tsconfig is CommonJS: {diags:?}"
        );
    }

    // Negative space for #7781: a `"type":"module"` package with a tsconfig silent
    // on module format is ESM (Node's default keys on the manifest `type`), where
    // extensions are required — so the rule must keep firing.
    #[test]
    fn still_flags_type_module_with_silent_tsconfig_issue_7781() {
        let pkg = r#"{"name":"pkg","type":"module"}"#;
        let tsconfig = r#"{"compilerOptions":{"rootDir":"src","outDir":"lib"}}"#;
        let diags = run_in_project(
            &[("package.json", pkg), ("tsconfig.json", tsconfig)],
            "import { x } from './util';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "type:module with silent tsconfig is ESM — extensions still required: {diags:?}"
        );
    }

    // Negative space for #7781: an explicit `module:esnext` tsconfig is a positive
    // ESM signal that the package.json-`type` fallback must NOT override, even
    // when the package.json omits `"type"`. Extensions stay required.
    #[test]
    fn still_flags_esnext_tsconfig_even_without_type_field_issue_7781() {
        let pkg = r#"{"name":"pkg","main":"dist/index.js"}"#;
        let tsconfig = r#"{"compilerOptions":{"module":"esnext"}}"#;
        let diags = run_in_project(
            &[("package.json", pkg), ("tsconfig.json", tsconfig)],
            "import { x } from './util';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "module:esnext is a positive ESM signal not overridden by package.json type fallback: {diags:?}"
        );
    }

    // Negative space: a plain Node ESM project (no bundler, no Angular) still
    // needs explicit extensions, so the rule must keep firing.
    #[test]
    fn still_flags_non_angular_node_esm_project() {
        let pkg = r#"{"type":"module","dependencies":{}}"#;
        let diags = run_in_project(
            &[("package.json", pkg)],
            "import { columns } from './makeData';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "non-Angular ESM project must still require extensions: {diags:?}"
        );
    }

    // Negative space for #2340: a multi-segment relative path with a non-`..`
    // final segment is a real extensionless file import and must stay flagged.
    #[test]
    fn still_flags_nested_extensionless_file_import() {
        let pkg = r#"{"type":"module","dependencies":{}}"#;
        let diags = run_in_project(
            &[("package.json", pkg)],
            "import x from '../bar/baz';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "nested extensionless file import must still be flagged: {diags:?}"
        );
    }

    // Regression for #4765: Vite resource-query imports (`?raw`, `?inline`,
    // `?worker`, `?url`) already carry a file extension before the `?` suffix —
    // the suffix is a bundler transform directive, not part of the path, so the
    // extension cannot be dropped. These must not be flagged.
    #[test]
    fn skips_vite_resource_query_imports_issue_4765() {
        let pkg = r#"{"type":"module","dependencies":{}}"#;
        let diags = run_in_project(
            &[("package.json", pkg)],
            "import App from './App.vue?raw';\n\
             import style from './style.css?inline';\n\
             import worker from './worker.js?worker';\n\
             import url from './img.png?url';\n",
        );
        assert!(
            diags.is_empty(),
            "Vite `?query` imports already carry an extension: {diags:?}"
        );
    }

    // Negative space for #4765: a plain extensionless import with no `?query`
    // suffix is still missing its extension and must stay flagged.
    #[test]
    fn still_flags_plain_extensionless_import_alongside_query_form() {
        let pkg = r#"{"type":"module","dependencies":{}}"#;
        let diags = run_in_project(
            &[("package.json", pkg)],
            "import foo from './foo';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "plain extensionless import (no `?query`) must still be flagged: {diags:?}"
        );
    }
}

#[cfg(test)]
mod prisma_output_tests {
    use super::Check;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    const SCHEMA_CUSTOM_OUTPUT: &str = "generator client {\n  \
        provider = \"prisma-client-js\"\n  \
        output   = \"./client\"\n}\n\n\
        datasource db {\n  provider = \"postgresql\"\n}\n";

    /// Build a temp Node-ESM tree (no bundler, so the rule is active) with the
    /// importer and an optional sibling `schema.prisma`, run the rule, and return
    /// its diagnostics. The generator `output` directory is never created.
    fn run_with_schema(
        importer_rel: &str,
        source: &str,
        schema_rel: &str,
        schema: Option<&str>,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
        if let Some(schema) = schema {
            let schema_path = dir.path().join(schema_rel);
            fs::create_dir_all(schema_path.parent().unwrap()).unwrap();
            fs::write(&schema_path, schema).unwrap();
        }
        let importer = dir.path().join(importer_rel);
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, source).unwrap();
        let canon = fs::canonicalize(&importer).unwrap();
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

    #[test]
    fn no_fp_for_custom_prisma_output_import_issue_2293() {
        // prisma/prisma reproducer: the extensionless import of the generated
        // client at `./client/edge` (the `generator { output = "./client" }`
        // directory) is resolved by the generated package's exports map, so the
        // missing-extension warning is a false positive.
        let source = "import { PrismaClient } from './client/edge';";
        let diags = run_with_schema(
            "packages/bundle-size/da-workers-pg/index.js",
            source,
            "packages/bundle-size/da-workers-pg/schema.prisma",
            Some(SCHEMA_CUSTOM_OUTPUT),
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn still_flags_extensionless_non_prisma_import_with_schema_present() {
        // Negative space: with the Prisma signal present, an extensionless import
        // outside the generator output dir still needs an explicit extension.
        let source = "import { makeData } from './makeData';";
        let diags = run_with_schema(
            "packages/bundle-size/da-workers-pg/index.js",
            source,
            "packages/bundle-size/da-workers-pg/schema.prisma",
            Some(SCHEMA_CUSTOM_OUTPUT),
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("makeData"));
    }

    #[test]
    fn still_flags_prisma_shaped_import_without_schema() {
        // No `schema.prisma` = no Prisma signal: `./client/edge` is an ordinary
        // extensionless import and must still be flagged.
        let source = "import { PrismaClient } from './client/edge';";
        let diags = run_with_schema(
            "packages/bundle-size/da-workers-pg/index.js",
            source,
            "packages/bundle-size/da-workers-pg/schema.prisma",
            None,
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("client/edge"));
    }
}
