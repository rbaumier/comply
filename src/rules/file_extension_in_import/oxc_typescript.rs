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
    KNOWN_EXTENSIONS.iter().any(|ext| spec.ends_with(ext))
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

fn project_uses_bundler(ctx: &CheckCtx) -> bool {
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

/// Angular's build toolchain (`@angular/build`/Angular CLI) resolves TypeScript
/// modules without explicit file extensions, so extensionless relative imports
/// are correct there. Detection walks up to the nearest package.json, so a
/// nested Angular example inside a non-Angular monorepo is recognised.
fn project_is_angular(ctx: &CheckCtx) -> bool {
    ctx.project
        .frameworks_for_path(ctx.path)
        .iter()
        .any(|fw| fw.name == "angular")
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
        // Both probes are directory-invariant (manifest + bundler config +
        // tsconfig chain), so memoize the combined skip decision per directory:
        // `has_bundler_config` stat-walks the ancestor tree, which is otherwise
        // re-run for every file in a deep monorepo.
        let skip_for_bundler = ctx.project.cached_bundler(ctx.path, || {
            project_uses_bundler(ctx)
                || tsconfig_uses_bundler_resolution(ctx)
                || project_is_angular(ctx)
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
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Relative import `{spec}` is missing a file extension. Add an explicit extension (e.g. `.js`, `.ts`) for ESM compatibility.",
        ),
        severity: Severity::Warning,
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

    // Regression for #1712: Angular's build toolchain resolves extensionless
    // relative imports, so the rule must stay silent when Angular is detected.

    #[test]
    fn skips_angular_project_detected_via_angular_json() {
        // examples/angular/filters carries angular.json but only @angular/build
        // in devDependencies (no @angular/core), matching the issue's repro.
        let pkg = r#"{"devDependencies":{"@angular/build":"^20.0.0","@angular/cli":"^20.0.0"}}"#;
        let diags = run_in_project(
            &[("package.json", pkg), ("angular.json", "{}")],
            "import { columnHelper, columns } from './makeData';\nimport TableFilter from './table-filter/table-filter';\n",
        );
        assert!(
            diags.is_empty(),
            "Angular project (angular.json) must not require file extensions: {diags:?}"
        );
    }

    #[test]
    fn skips_angular_project_detected_via_angular_build_dep() {
        let pkg = r#"{"devDependencies":{"@angular/build":"^20.0.0"}}"#;
        let diags = run_in_project(
            &[("package.json", pkg)],
            "import { columns } from './makeData';\n",
        );
        assert!(
            diags.is_empty(),
            "@angular/build dependency must suppress the rule: {diags:?}"
        );
    }

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
}
