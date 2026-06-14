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
    ".js", ".ts", ".tsx", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json", ".css", ".scss",
    ".less", ".svg", ".png", ".vue", ".svelte",
];

/// Bundlers and runtimes that resolve extensionless imports natively. Their
/// presence in `package.json` deps means the rule must stay silent.
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

/// True when the project depends on a bundler/runtime that resolves
/// extensionless imports natively. Walks up to the nearest `package.json` and
/// checks every dep section.
fn project_uses_bundler(ctx: &crate::rules::backend::CheckCtx) -> bool {
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

/// Returns `true` only when the project context positively requires explicit
/// file extensions — i.e. Node ESM without a bundler.
///
/// Inverted default: the rule stays silent unless it can prove the project is
/// running native Node ESM (`moduleResolution: node16`/`nodenext`, or
/// `module: node16`/`nodenext`, or `package.json` `"type":"module"` with no
/// bundler present). Every other mode (`node`, `node10`, `classic`, `bundler`,
/// `commonjs`, or absent config) accepts extensionless imports and/or rejects
/// explicit `.ts` extensions.
fn requires_explicit_extension(ctx: &crate::rules::backend::CheckCtx) -> bool {
    use crate::project::ModuleType;

    if project_uses_bundler(ctx) {
        return false;
    }

    if let Some(ts) = ctx.project.nearest_tsconfig(ctx.path) {
        if let Some(mr) = ts.module_resolution.as_deref() {
            if ["node", "node10", "classic", "bundler"]
                .iter()
                .any(|&m| mr.eq_ignore_ascii_case(m))
            {
                return false;
            }
            if ["node16", "nodenext"]
                .iter()
                .any(|&m| mr.eq_ignore_ascii_case(m))
            {
                return true;
            }
        }
        if let Some(m) = ts.module.as_deref() {
            if m.eq_ignore_ascii_case("commonjs") {
                return false;
            }
            if ["node16", "nodenext"]
                .iter()
                .any(|&v| m.eq_ignore_ascii_case(v))
            {
                return true;
            }
        }
    }

    // Fallback: package.json "type":"module" without a bundler = native Node ESM.
    ctx.project
        .nearest_package_json(ctx.path)
        .map(|pkg| pkg.module_type == ModuleType::Module)
        .unwrap_or(false)
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if !requires_explicit_extension(ctx) { return; }

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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &canon, &project, crate::rules::file_ctx::default_static_file_ctx())
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &canon, &project, crate::rules::file_ctx::default_static_file_ctx())
    }

    // -------- baseline (no project context — empty ProjectCtx, no package.json) --------
    // With the inverted default, run_on() (no package.json) → silence.
    // Tests that need the rule to fire must use run_with_project(Some(r#"{"type":"module"}"#), ...).

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
        assert!(
            d.is_empty(),
            "webpack resolves extensionless imports: {d:?}"
        );
    }

    #[test]
    fn skips_when_esbuild_present() {
        let pkg = r#"{"devDependencies":{"esbuild":"^0.20"}}"#;
        let d = run_with_project(Some(pkg), None, "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "esbuild resolves extensionless imports: {d:?}"
        );
    }

    #[test]
    fn skips_when_vite_config_present() {
        let d = run_with_config_file("vite.config.ts", "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "vite config resolves extensionless imports: {d:?}"
        );
    }

    #[test]
    fn skips_when_next_config_present() {
        let d = run_with_config_file("next.config.js", "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "next config resolves extensionless imports: {d:?}"
        );
    }

    #[test]
    fn skips_when_vitest_config_present() {
        let d = run_with_config_file("vitest.config.ts", "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "vitest config → bundler context → silence: {d:?}"
        );
    }

    #[test]
    fn skips_when_tsconfig_module_resolution_bundler() {
        let tsc = r#"{"compilerOptions":{"moduleResolution":"bundler"}}"#;
        let d = run_with_project(None, Some(tsc), "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "tsconfig moduleResolution=bundler should skip: {d:?}"
        );
    }

    #[test]
    fn skips_when_module_is_bundler_no_esm_context() {
        let tsc = r#"{"compilerOptions":{"module":"bundler"}}"#;
        let d = run_with_project(None, Some(tsc), "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "module:bundler sans package.json type:module → silence (pas d'ESM prouvé): {d:?}"
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

    // -------- FP regression: extensionless-accepting resolution modes --------

    #[test]
    fn skips_when_module_resolution_node() {
        let tsc = r#"{"compilerOptions":{"moduleResolution":"node"}}"#;
        let d = run_with_project(None, Some(tsc), "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "moduleResolution:node resolves extensionless → silence: {d:?}"
        );
    }

    #[test]
    fn skips_when_module_resolution_node10() {
        let tsc = r#"{"compilerOptions":{"moduleResolution":"node10"}}"#;
        let d = run_with_project(None, Some(tsc), "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "moduleResolution:node10 resolves extensionless → silence: {d:?}"
        );
    }

    #[test]
    fn skips_when_module_resolution_classic() {
        let tsc = r#"{"compilerOptions":{"moduleResolution":"classic"}}"#;
        let d = run_with_project(None, Some(tsc), "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "moduleResolution:classic resolves extensionless → silence: {d:?}"
        );
    }

    #[test]
    fn skips_when_module_commonjs() {
        let tsc = r#"{"compilerOptions":{"module":"commonjs"}}"#;
        let d = run_with_project(None, Some(tsc), "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "module:commonjs resolves extensionless → silence: {d:?}"
        );
    }

    #[test]
    fn skips_when_node16_but_bundler_present() {
        let tsc = r#"{"compilerOptions":{"moduleResolution":"node16"}}"#;
        let pkg = r#"{"devDependencies":{"vite":"^5"}}"#;
        let d = run_with_project(Some(pkg), Some(tsc), "import { foo } from './utils';");
        assert!(
            d.is_empty(),
            "node16 + bundler present → bundler wins → silence: {d:?}"
        );
    }

    // -------- positive: native Node ESM modes that require extensions --------

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

    // -------- extends chain: moduleResolution inherited from base tsconfig --------

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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &canon, &project, crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn skips_when_module_resolution_bundler_in_extended_tsconfig() {
        let base = r#"{"compilerOptions":{"moduleResolution":"bundler"}}"#;
        let child = r#"{"extends":"../tsconfig.base.json"}"#;
        let d = run_with_extends_tsconfig(
            base,
            child,
            "import { foo } from './utils';",
        );
        assert!(
            d.is_empty(),
            "moduleResolution:bundler inherited via extends → silence: {d:?}"
        );
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
