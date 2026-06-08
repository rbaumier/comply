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
    spec.ends_with('/') || spec.ends_with("/index")
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
            project_uses_bundler(ctx) || tsconfig_uses_bundler_resolution(ctx)
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
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::{run_oxc_ts, run_oxc_ts_with_project};
    use std::fs;
    use tempfile::TempDir;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts(source, &Check)
    }


    fn run_with_project(
        pkg_json: Option<&str>,
        tsconfig_json: Option<&str>,
        source: &str,
    ) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        if let Some(pj) = pkg_json {
            fs::write(dir.path().join("package.json"), pj).unwrap();
        }
        if let Some(tc) = tsconfig_json {
            fs::write(dir.path().join("tsconfig.json"), tc).unwrap();
        }
        let file_path = dir.path().join("src/server.ts");
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
        run_oxc_ts_with_project(source, &Check, &project)
    }


    fn run_with_config_file(config_file: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(config_file), "").unwrap();
        let file_path = dir.path().join("src/server.ts");
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
        run_oxc_ts_with_project(source, &Check, &project)
    }


    fn run_with_extends_tsconfig(
        base_tsconfig: &str,
        child_tsconfig_extends: &str,
        source: &str,
    ) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("tsconfig.base.json"), base_tsconfig).unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("tsconfig.json"), child_tsconfig_extends).unwrap();
        let file_path = src.join("server.ts");
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
        run_oxc_ts_with_project(source, &Check, &project)
    }


    #[test]
    fn flags_relative_import_without_extension() {
        let pkg = r#"{"type":"module"}"#;
        let d = run_with_project(Some(pkg), None, "import { foo } from './utils';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_relative_import_with_extension() {
        assert!(run_on("import { foo } from './utils.js';").is_empty());
    }


    #[test]
    fn allows_ts_extension() {
        assert!(run_on("import { foo } from './utils.ts';").is_empty());
    }


    #[test]
    fn skips_bare_specifier() {
        assert!(run_on("import React from 'react';").is_empty());
    }


    #[test]
    fn flags_parent_relative_without_extension() {
        let pkg = r#"{"type":"module"}"#;
        let d = run_with_project(Some(pkg), None, "import { bar } from '../helpers/bar';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_json_extension() {
        assert!(run_on("import data from './config.json';").is_empty());
    }


    #[test]
    fn flags_reexport_without_extension() {
        let pkg = r#"{"type":"module"}"#;
        let d = run_with_project(Some(pkg), None, "export { foo } from './utils';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn skips_node_protocol() {
        assert!(run_on("import fs from 'node:fs';").is_empty());
    }


    #[test]
    fn skips_scoped_bare_specifier() {
        assert!(run_on("import x from '@scope/pkg';").is_empty());
    }


    #[test]
    fn skips_directory_import_trailing_slash() {
        assert!(run_on("import x from './components/';").is_empty());
    }


    #[test]
    fn skips_directory_import_index() {
        assert!(run_on("import x from './components/index';").is_empty());
    }


    #[test]
    fn allows_tsx_extension() {
        assert!(run_on("import Btn from './Button.tsx';").is_empty());
    }


    #[test]
    fn skips_dynamic_import() {
        // Dynamic imports are call_expression nodes, not import_statement.
        assert!(run_on("const m = import('./utils');").is_empty());
    }


    #[test]
    fn flags_when_node_esm_without_bundler() {
        let pkg = r#"{"type":"module","dependencies":{}}"#;
        let d = run_with_project(Some(pkg), None, "import { foo } from './utils';");
        assert_eq!(
            d.len(),
            1,
            "Node ESM without a bundler still needs explicit extensions"
        );
    }


    #[test]
    fn flags_when_module_resolution_node16() {
        let tsc = r#"{"compilerOptions":{"moduleResolution":"node16"}}"#;
        let d = run_with_project(None, Some(tsc), "import { foo } from './utils';");
        assert_eq!(
            d.len(),
            1,
            "moduleResolution:node16 requires explicit extensions"
        );
    }


    #[test]
    fn flags_when_module_resolution_nodenext() {
        let tsc = r#"{"compilerOptions":{"moduleResolution":"nodenext"}}"#;
        let d = run_with_project(None, Some(tsc), "import { foo } from './utils';");
        assert_eq!(
            d.len(),
            1,
            "moduleResolution:nodenext requires explicit extensions"
        );
    }


    #[test]
    fn flags_when_module_node16() {
        let tsc = r#"{"compilerOptions":{"module":"node16"}}"#;
        let d = run_with_project(None, Some(tsc), "import { foo } from './utils';");
        assert_eq!(d.len(), 1, "module:node16 requires explicit extensions");
    }


    #[test]
    fn flags_when_module_nodenext() {
        let tsc = r#"{"compilerOptions":{"module":"nodenext"}}"#;
        let d = run_with_project(None, Some(tsc), "import { foo } from './utils';");
        assert_eq!(d.len(), 1, "module:nodenext requires explicit extensions");
    }


    #[test]
    fn flags_when_module_resolution_node16_in_extended_tsconfig() {
        let base = r#"{"compilerOptions":{"moduleResolution":"node16"}}"#;
        let child = r#"{"extends":"../tsconfig.base.json"}"#;
        let d = run_with_extends_tsconfig(
            base,
            child,
            "import { foo } from './utils';",
        );
        assert_eq!(
            d.len(),
            1,
            "moduleResolution:node16 inherited via extends → must flag"
        );
    }
}
