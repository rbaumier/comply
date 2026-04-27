//! file-extension-in-import backend — flag relative imports missing a file extension.
//!
//! Walks the program's top-level `import_statement` and `export_statement`
//! (re-export) nodes, extracts the source specifier, and flags relative
//! specifiers (`./` or `../`) that do not end in a known extension and are
//! not directory-style imports (trailing `/` or `/index`).
//!
//! Scope: only fires for projects that need explicit extensions — Node ESM
//! without a bundler, or Deno. Skipped when a bundler (Vite, Webpack, esbuild,
//! Parcel, Rollup) or Bun is present, or when `tsconfig.json` declares
//! `moduleResolution: "bundler"`. Adding `.ts` extensions in those projects
//! breaks TypeScript imports and yields pure noise.

use crate::diagnostic::{Diagnostic, Severity};

const KNOWN_EXTENSIONS: &[&str] = &[
    ".js", ".ts", ".tsx", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json",
    ".css", ".scss", ".less", ".svg", ".png", ".vue", ".svelte",
];

/// Bundlers and runtimes that resolve extensionless imports natively. Their
/// presence in `package.json` deps means the rule must stay silent.
const BUNDLER_DEPS: &[&str] = &[
    "vite",
    "webpack",
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

fn has_known_extension(spec: &str) -> bool {
    KNOWN_EXTENSIONS.iter().any(|ext| spec.ends_with(ext))
}

fn is_directory_import(spec: &str) -> bool {
    spec.ends_with('/') || spec.ends_with("/index")
}

fn is_relative(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../")
}

/// True when the project depends on a bundler/runtime that resolves
/// extensionless imports natively. Walks up to the nearest `package.json` and
/// checks every dep section.
fn project_uses_bundler(ctx: &crate::rules::backend::CheckCtx) -> bool {
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
        return false;
    };
    BUNDLER_DEPS.iter().any(|dep| pkg.has_dep_or_engine(dep))
}

/// True when `tsconfig.json` declares `moduleResolution: "bundler"` (or any
/// resolution mode that lets TS resolve extensionless specifiers — `bundler`,
/// `node16`, `nodenext` all accept them with the right `noEmit`/`allowImportingTsExtensions`
/// pairing, but `bundler` is the unambiguous "I have a bundler" signal).
fn tsconfig_uses_bundler_resolution(ctx: &crate::rules::backend::CheckCtx) -> bool {
    let Some(ts) = ctx.project.nearest_tsconfig(ctx.path) else {
        return false;
    };
    ts.module
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case("bundler"))
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if project_uses_bundler(ctx) { return; }
    if tsconfig_uses_bundler_resolution(ctx) { return; }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        if kind != "import_statement" && kind != "export_statement" {
            continue;
        }
        let Some(source_node) = child.child_by_field_name("source") else {
            continue;
        };
        let Ok(raw) = std::str::from_utf8(&source[source_node.byte_range()]) else {
            continue;
        };
        let spec = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');

        if !is_relative(spec) {
            continue;
        }
        if has_known_extension(spec) {
            continue;
        }
        if is_directory_import(spec) {
            continue;
        }

        let pos = source_node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "file-extension-in-import".into(),
            message: format!(
                "Relative import `{spec}` is missing a file extension. Add an explicit extension (e.g. `.js`, `.ts`) for ESM compatibility.",
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::{run_ts, run_ts_with_project_and_path};
    use std::fs;
    use tempfile::TempDir;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        run_ts(source, &Check)
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
        run_ts_with_project_and_path(source, &Check, &project, &canon)
    }

    // -------- baseline (no project context — empty ProjectCtx, no bundler) --------

    #[test]
    fn flags_relative_import_without_extension() {
        let d = run_on("import { foo } from './utils';");
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
        let d = run_on("import { bar } from '../helpers/bar';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_json_extension() {
        assert!(run_on("import data from './config.json';").is_empty());
    }

    #[test]
    fn flags_reexport_without_extension() {
        let d = run_on("export { foo } from './utils';");
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

    // -------- bundler / runtime gating --------

    #[test]
    fn skips_when_vite_present() {
        let pkg = r#"{"devDependencies":{"vite":"^5"}}"#;
        let d = run_with_project(Some(pkg), None, "import { foo } from './utils';");
        assert!(d.is_empty(), "vite resolves extensionless imports: {d:?}");
    }

    #[test]
    fn skips_when_bun_present() {
        let pkg = r#"{"dependencies":{"bun":"^1"}}"#;
        let d = run_with_project(Some(pkg), None, "import { foo } from './utils';");
        assert!(d.is_empty(), "bun resolves extensionless imports: {d:?}");
    }

    #[test]
    fn skips_when_webpack_present() {
        let pkg = r#"{"devDependencies":{"webpack":"^5"}}"#;
        let d = run_with_project(Some(pkg), None, "import { foo } from './utils';");
        assert!(d.is_empty(), "webpack resolves extensionless imports: {d:?}");
    }

    #[test]
    fn skips_when_esbuild_present() {
        let pkg = r#"{"devDependencies":{"esbuild":"^0.20"}}"#;
        let d = run_with_project(Some(pkg), None, "import { foo } from './utils';");
        assert!(d.is_empty(), "esbuild resolves extensionless imports: {d:?}");
    }

    #[test]
    fn skips_when_tsconfig_module_resolution_bundler() {
        let tsc = r#"{"compilerOptions":{"module":"bundler"}}"#;
        let d = run_with_project(None, Some(tsc), "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "tsconfig moduleResolution=bundler should skip: {d:?}"
        );
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
}
