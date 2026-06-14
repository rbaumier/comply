//! import-no-unresolved OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let index = ctx.project.import_index();
        if index.is_empty() {
            return Vec::new();
        }

        // Scaffold template files (create-t3-app's `cli/template/`, etc.) are
        // assembled into the generated project at scaffold time; their cross-file
        // relative imports resolve only after that assembly. In the unassembled
        // tree those siblings are absent, so the imports are not real errors.
        if crate::rules::path_utils::is_scaffold_template_path(ctx.path) {
            return Vec::new();
        }

        let canon = index.canonical(ctx.path);
        let mut seen: HashSet<(String, usize)> = HashSet::new();
        let mut diagnostics = Vec::new();

        for imp in index.get_imports(&canon) {
            let is_relative = imp.specifier.starts_with("./") || imp.specifier.starts_with("../");
            if !is_relative {
                continue;
            }
            if imp.source_path.is_some() {
                continue;
            }
            // Drop a build-tool query/hash suffix (`./file.txt?url`,
            // `./worker.js?worker`) before any on-disk check: Vite et al. attach
            // the import directive as a query the bundler consumes at build time;
            // only the bare path exists on disk.
            let fs_specifier = crate::rules::path_utils::strip_specifier_query(&imp.specifier);
            // Skip gitignored build-time generated files (e.g. TanStack
            // Router's `routeTree.gen.ts`) and imports into build-output /
            // codegen directories (dist/build/out, generated/__generated__/
            // .prisma/prisma/gen, node_modules): often absent at lint time,
            // always present at build/dev time.
            if crate::rules::path_utils::is_generated_file_specifier(&imp.specifier)
                || crate::rules::path_utils::is_build_output_specifier(&imp.specifier)
            {
                continue;
            }
            // An import resolving into the nearest tsconfig's
            // `compilerOptions.outDir` (e.g. pnpm's `outDir: lib`) targets
            // compiled output: gitignored and absent in a clean checkout, so the
            // import is expected to be unresolved at lint time.
            if resolves_into_out_dir(ctx, &imp.specifier) {
                continue;
            }
            // CSS, CSS Modules, SVG, and other static assets are imported via
            // build-tool support (Webpack, Vite, Next.js) and never enter the
            // TS/JS index. When such a non-source file exists on disk next to
            // the importer, the import is resolved — don't flag it.
            if is_existing_asset_import(ctx.path, fs_specifier) {
                continue;
            }
            // A relative import whose target source file exists on disk but lives
            // in a directory excluded from the scan (e.g. vendored code under
            // `vendor/`) is absent from the import index, so `source_path` is
            // `None` — yet the import is genuinely resolvable. Don't flag it.
            if is_existing_source_import(ctx.path, fs_specifier) {
                continue;
            }
            // Angular schematics/builders generate a `schema.ts` TypeScript
            // interface from a sibling `schema.json` at build time (via
            // `json-schema-to-typescript`). The `.ts` is gitignored and absent
            // in a clean checkout, but the `.json` source of truth sits next to
            // the importer at the same base path — treat that as resolved.
            if has_generated_json_sibling(ctx.path, fs_specifier) {
                continue;
            }
            if !seen.insert((imp.specifier.clone(), imp.line)) {
                continue;
            }

            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: imp.line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Unable to resolve import path `{}` — file does not exist.",
                    imp.specifier
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

/// Source extensions resolved through the TS/JS import index. A specifier
/// carrying one of these is a real source import — if it stayed unresolved,
/// the target is genuinely missing and must be flagged.
const SOURCE_EXTS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "mjs", "cts", "cjs", "vue"];

/// True for a relative specifier that names a non-source file (a `.css`,
/// `.svg`, `.png`, … asset) present on disk next to the importer. These never
/// enter the TS/JS index, so `source_path` is always `None`, yet the import is
/// resolved at build time.
pub(super) fn is_existing_asset_import(importer: &Path, specifier: &str) -> bool {
    let Some(ext) = Path::new(specifier).extension().and_then(|e| e.to_str()) else {
        return false;
    };
    if SOURCE_EXTS.contains(&ext) {
        return false;
    }
    let Some(base_dir) = importer.parent() else {
        return false;
    };
    base_dir.join(specifier).is_file()
}

/// True for a relative specifier that resolves to a real source file on disk,
/// even when that file is absent from the import index because it lives in a
/// directory excluded from the scan (e.g. vendored code under `vendor/`).
/// Mirrors the import index's resolution order — bare path, each source
/// extension, then `index.<ext>` — but checks the filesystem directly instead
/// of the in-memory `known` set. A directory target additionally resolves via
/// that directory's `package.json` `main`/`exports` entry (Node/TypeScript
/// directory resolution). A specifier with no matching file on disk
/// (e.g. `./does-not-exist`) returns `false` and is still flagged.
pub(super) fn is_existing_source_import(importer: &Path, specifier: &str) -> bool {
    let Some(base_dir) = importer.parent() else {
        return false;
    };
    let raw = base_dir.join(specifier);

    if Path::new(specifier)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| SOURCE_EXTS.contains(&ext))
        && raw.is_file()
    {
        return true;
    }
    if SOURCE_EXTS.iter().any(|ext| raw.with_extension(ext).is_file()) {
        return true;
    }
    if SOURCE_EXTS
        .iter()
        .any(|ext| raw.join(format!("index.{ext}")).is_file())
    {
        return true;
    }
    directory_resolves_via_package_json(&raw)
}

/// True when `dir` is a directory whose `package.json` declares a `main` or
/// `exports` entry that points to an existing file. Mirrors Node/TypeScript
/// directory resolution: `require('../..')` / `import x from '../'` resolves to
/// the target directory's package entry point. [`PackageJson::entry_files`]
/// already collects the `main` value and every `exports` target (including the
/// conditional `import`/`require` variants), manifest-dir-relative with forward
/// slashes, so each entry joins directly onto `dir`. A directory with no
/// `package.json`, an unparseable one, or one whose entries name no on-disk
/// file returns `false`.
///
/// [`PackageJson::entry_files`]: crate::project::PackageJson::entry_files
fn directory_resolves_via_package_json(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let Ok(raw) = std::fs::read_to_string(dir.join("package.json")) else {
        return false;
    };
    let Some(pkg) = crate::project::PackageJson::parse(&raw) else {
        return false;
    };
    pkg.entry_files.iter().any(|entry| dir.join(entry).is_file())
}

/// True for an extensionless relative specifier (e.g. `./schema`,
/// `../workspace/schema`) whose target has a sibling `.json` file at the same
/// base path on disk (e.g. `schema.json`). This is the Angular schematics/
/// builders codegen convention: the `.ts` types are generated from the
/// `schema.json` source of truth at build time and are absent in a clean
/// checkout. A specifier that already carries an extension, or one with no
/// matching `.json` on disk, returns `false` and is still flagged.
pub(super) fn has_generated_json_sibling(importer: &Path, specifier: &str) -> bool {
    if Path::new(specifier).extension().is_some() {
        return false;
    }
    let Some(base_dir) = importer.parent() else {
        return false;
    };
    base_dir.join(specifier).with_extension("json").is_file()
}

/// Resolve a path lexically — collapse `.`/`..` segments without touching the
/// filesystem, since the tsconfig `outDir` is gitignored and absent in a clean
/// checkout. `..` pops the last normal segment; a `..` with nothing left to pop
/// is preserved.
fn normalize_lexical(path: &Path) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// True when `specifier`, resolved relative to the importer, lands inside the
/// compiled-output directory declared by the nearest tsconfig's
/// `compilerOptions.outDir`. Lexical comparison only — the outDir is absent in a
/// clean checkout, so canonicalizing would fail.
fn resolves_into_out_dir(ctx: &CheckCtx, specifier: &str) -> bool {
    let Some(out_dir) = ctx.project.tsconfig_out_dir(ctx.path) else {
        return false;
    };
    let Some(base_dir) = ctx.path.parent() else {
        return false;
    };
    let resolved = normalize_lexical(&base_dir.join(specifier));
    resolved.starts_with(normalize_lexical(&out_dir))
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod oxc_tests {
    use crate::rules::path_utils::{is_build_output_specifier, is_generated_file_specifier};

    #[test]
    fn detects_generated_specifiers_issue_487() {
        assert!(is_generated_file_specifier("./routeTree.gen"));
        assert!(is_generated_file_specifier("./routeTree.gen.ts"));
        assert!(is_generated_file_specifier("../app/routeTree.gen"));
        assert!(!is_generated_file_specifier("./routeTree"));
        assert!(!is_generated_file_specifier("./generated"));
    }

    #[test]
    fn detects_build_output_specifiers_issue_1005() {
        // reproducers from the issue
        assert!(is_build_output_specifier("../../../dist/cjs/index.js"));
        assert!(is_build_output_specifier("../../dist/esm/index.js"));
        assert!(is_build_output_specifier("../build/index.js"));
        assert!(is_build_output_specifier("./out/index.js"));
        // still flagged — real source / not an exact build segment
        assert!(!is_build_output_specifier("./src/index.js"));
        assert!(!is_build_output_specifier("../distance/index.js"));
        assert!(!is_build_output_specifier("./distribution/x"));
        assert!(!is_build_output_specifier("./lib/util.js")); // lib intentionally NOT skipped
    }

    #[test]
    fn detects_generated_dir_specifiers_issue_1420() {
        // reproducers from the issue (Prisma / GraphQL codegen output); the
        // generated-dir set is now part of `is_build_output_specifier`.
        assert!(is_build_output_specifier("./generated/prisma/client"));
        assert!(is_build_output_specifier("./generated/client"));
        assert!(is_build_output_specifier("./node_modules/@prisma/client"));
        assert!(is_build_output_specifier("../src/__generated__/graphql"));
        assert!(is_build_output_specifier("./.prisma/client"));
        // still flagged — a genuinely broken relative import has no codegen segment
        assert!(!is_build_output_specifier("./does-not-exist"));
        assert!(!is_build_output_specifier("../utils/helper"));
        assert!(!is_build_output_specifier("./generated-things")); // substring, not a segment
    }
}

#[cfg(test)]
mod out_dir_tests {
    use super::Check;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    fn run_with_tsconfig(
        importer_rel: &str,
        source: &str,
        tsconfig_rel: &str,
        tsconfig: &str,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        let ts_path = dir.path().join(tsconfig_rel);
        fs::create_dir_all(ts_path.parent().unwrap()).unwrap();
        fs::write(&ts_path, tsconfig).unwrap();
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
    fn no_fp_for_tsconfig_out_dir_import_issue_1972() {
        // pnpm reproducer: a package whose tsconfig declares `outDir: lib`. A
        // test imports compiled output under `lib/`, gitignored and absent in a
        // clean checkout, so the import must not be flagged.
        let source = "import type { NodeId } from '../lib/nextNodeId.js';";
        let diags = run_with_tsconfig(
            "deps-resolver/test/dedupeDepPaths.test.ts",
            source,
            "deps-resolver/tsconfig.json",
            r#"{"compilerOptions":{"outDir":"lib","rootDir":"src"}}"#,
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn still_flags_missing_lib_import_without_out_dir_issue_1005() {
        // A project whose tsconfig does NOT declare `outDir: lib` keeps `lib/` as
        // real source: a missing `./lib/util.js` is still a genuine broken import.
        let source = "import { util } from './lib/util.js';";
        let diags = run_with_tsconfig(
            "pkg/app.ts",
            source,
            "pkg/tsconfig.json",
            r#"{"compilerOptions":{"rootDir":"src"}}"#,
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("lib/util.js"));
    }
}

#[cfg(test)]
mod scaffold_template_tests {
    use super::Check;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    fn run_in_dir(importer_rel: &str, source: &str) -> Vec<crate::diagnostic::Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
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
    fn no_fp_for_scaffold_template_file_issue_1753() {
        // create-t3-app reproducer: a config template under `cli/template/`
        // imports `./src/env.js`, a sibling that only exists after the CLI
        // assembles the generated project. Until then the path is missing, so
        // the import must not be flagged.
        let source = "import './src/env.js';";
        let diags = run_in_dir("cli/template/extras/config/next-config-appdir.js", source);
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn still_flags_missing_import_outside_template_dir() {
        // The same missing import in normal (non-template) source is a real
        // error and must still fire — the exemption stays narrow.
        let source = "import './src/env.js';";
        let diags = run_in_dir("config/next-config-appdir.js", source);
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("src/env.js"));
    }
}

#[cfg(test)]
mod angular_schema_json_tests {
    use super::Check;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Build a temp tree with the importer plus optional sibling `.json` files,
    /// run the rule on the importer, and return its diagnostics. The generated
    /// `.ts` types are never written — they don't exist in a clean checkout.
    fn run_with_siblings(
        importer_rel: &str,
        source: &str,
        sibling_jsons: &[&str],
    ) -> Vec<crate::diagnostic::Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        let importer = dir.path().join(importer_rel);
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, source).unwrap();
        for json_rel in sibling_jsons {
            let json = dir.path().join(json_rel);
            fs::create_dir_all(json.parent().unwrap()).unwrap();
            fs::write(&json, r#"{"$schema":"http://json-schema.org/schema"}"#).unwrap();
        }
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
    fn no_fp_for_angular_schematic_schema_import_issue_1740() {
        // angular/angular-cli reproducer: an ng-add schematic imports
        // `./schema`, whose `schema.ts` is generated from the sibling
        // `schema.json` at build time and absent in a clean checkout.
        let source = "import { externalSchematic } from '@angular-devkit/schematics';\n\
                      import { Schema as SSROptions } from './schema';\n";
        let diags = run_with_siblings(
            "packages/angular/ssr/schematics/ng-add/index.ts",
            source,
            &["packages/angular/ssr/schematics/ng-add/schema.json"],
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn no_fp_for_nested_schema_import_issue_1740() {
        // The same pattern via a nested relative path (`../workspace/schema`),
        // resolving to a `schema.json` in a sibling directory.
        let source = "import { Schema } from '../workspace/schema';\n";
        let diags = run_with_siblings(
            "packages/schematics/angular/application/index.ts",
            source,
            &["packages/schematics/angular/workspace/schema.json"],
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn still_flags_missing_import_without_json_sibling_issue_1740() {
        // No `schema.json` next to the importer: a genuinely broken `./schema`
        // import must still fire — the exemption stays narrow.
        let source = "import { Schema } from './schema';\n";
        let diags = run_with_siblings(
            "packages/angular/ssr/schematics/ng-add/index.ts",
            source,
            &[],
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("./schema"));
    }

    #[test]
    fn still_flags_explicit_ts_specifier_with_json_sibling_issue_1740() {
        // An explicit extension (`./schema.ts`) names a concrete source file; a
        // sibling `schema.json` does not excuse its absence — still flagged.
        let source = "import { Schema } from './schema.ts';\n";
        let diags = run_with_siblings(
            "packages/angular/ssr/schematics/ng-add/index.ts",
            source,
            &["packages/angular/ssr/schematics/ng-add/schema.json"],
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("schema.ts"));
    }

    #[test]
    fn no_fp_for_bare_specifier_resolving_to_dts_issue_1638() {
        // playwright reproducer: `import './types'` where the only file on disk
        // is `types.d.ts` (a declaration-only file, excluded from the index).
        // TypeScript resolves the bare extensionless specifier to its `.d.ts`
        // sibling, so the import must not be flagged.
        let source = "import type { HTMLReport } from './types';\n";
        let diags = run_with_siblings(
            "packages/html-reporter/src/index.tsx",
            source,
            &["packages/html-reporter/src/types.d.ts"],
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn still_flags_missing_bare_specifier_without_dts_issue_1638() {
        // A bare extensionless specifier with no source OR declaration sibling on
        // disk is a genuine broken import and must still fire.
        let source = "import type { T } from './nope';\n";
        let diags = run_with_siblings("packages/html-reporter/src/index.tsx", source, &[]);
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("./nope"));
    }
}

#[cfg(test)]
mod generated_target_tests {
    use super::Check;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// Build a temp svelte-like tree (importer + committed generated target),
    /// run the rule on the importer, and return its diagnostics. `index_target`
    /// controls whether the generated file is included in the import index —
    /// `false` simulates a diff-based scan (or a future scan-set exclusion)
    /// where the committed generated file is on disk but absent from
    /// `known_paths`.
    fn run_svelte(specifier: &str, target_rel: &str, index_target: bool) -> Vec<crate::diagnostic::Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"svelte"}"#).unwrap();

        let target = dir.path().join(target_rel);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(
            &target,
            "/* This file is generated by scripts/process-messages. Do not edit! */\nexport const w = 1;\n",
        )
        .unwrap();

        let importer = dir.path().join("packages/svelte/src/transition/index.js");
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        let src = format!("import * as w from '{specifier}';\n");
        fs::write(&importer, &src).unwrap();

        let importer_canon = fs::canonicalize(&importer).unwrap();
        let f_importer = SourceFile {
            path: importer_canon.clone(),
            language: Language::JavaScript,
        };
        let target_canon = fs::canonicalize(&target).unwrap();
        let f_target = SourceFile {
            path: target_canon,
            language: Language::JavaScript,
        };
        let refs: Vec<&SourceFile> = if index_target {
            vec![&f_importer, &f_target]
        } else {
            vec![&f_importer]
        };
        let project = ProjectCtx::for_test_with_files(&refs);
        run(&importer_canon, &src, &project)
    }

    fn run(path: &Path, src: &str, project: &ProjectCtx) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            path,
            project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn no_fp_when_generated_target_absent_from_index_issue_1759() {
        // svelte reproducer: `transition/index.js` imports the committed,
        // generated `warnings.js` (first line "Do not edit!"). When the target
        // is excluded from the import index, the on-disk existence check must
        // still resolve it — the import is not a real error.
        let diags = run_svelte(
            "../internal/client/warnings.js",
            "packages/svelte/src/internal/client/warnings.js",
            false,
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn no_fp_for_generated_sibling_errors_js_issue_1759() {
        // The sibling `errors.js`, same shape, imported from the same importer.
        let diags = run_svelte(
            "../internal/client/errors.js",
            "packages/svelte/src/internal/client/errors.js",
            false,
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn no_fp_when_generated_target_indexed_issue_1759() {
        // Full scan: the generated target is on disk and in the index. Must be
        // clean too — the resolver finds it via `known_paths`.
        let diags = run_svelte(
            "../internal/client/warnings.js",
            "packages/svelte/src/internal/client/warnings.js",
            true,
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn still_flags_genuinely_missing_target_issue_1759() {
        // The conservative boundary: when no file exists on disk at the resolved
        // path, the import is a real broken reference and must still fire.
        let diags = run_svelte(
            "../internal/client/does-not-exist.js",
            "packages/svelte/src/internal/client/warnings.js",
            false,
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("does-not-exist.js"));
    }
}

#[cfg(test)]
mod vite_query_string_tests {
    use super::Check;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Build a temp tree with the importer plus on-disk asset files, run the rule
    /// on the importer, and return its diagnostics.
    fn run_with_assets(
        importer_rel: &str,
        source: &str,
        assets: &[&str],
    ) -> Vec<crate::diagnostic::Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        let importer = dir.path().join(importer_rel);
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, source).unwrap();
        for asset_rel in assets {
            let asset = dir.path().join(asset_rel);
            fs::create_dir_all(asset.parent().unwrap()).unwrap();
            fs::write(&asset, "asset contents").unwrap();
        }
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
    fn no_fp_for_vite_query_string_asset_import_issue_1582() {
        // sveltejs/kit reproducer: Vite asset imports carry a `?url` (or
        // `?raw`/`?inline`/`?worker`/`?base64`) directive. The file exists on
        // disk; the query is a build-time transform, not part of the path, so the
        // import must not be flagged.
        let source = "import file from './file.txt?url';";
        let diags = run_with_assets(
            "src/routes/read/+server.js",
            source,
            &["src/routes/read/file.txt"],
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");

        // Bracketed route-param filenames with the same suffix (kit `basics`).
        let url = "import url from './[url].txt?url';";
        let diags = run_with_assets(
            "src/routes/read-file/+page.server.js",
            url,
            &["src/routes/read-file/[url].txt"],
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");

        let styles = "import styles from './[styles].css?url';";
        let diags = run_with_assets(
            "src/routes/read-file/+page.server.js",
            styles,
            &["src/routes/read-file/[styles].css"],
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn still_flags_missing_vite_query_string_asset_issue_1582() {
        // Negative space: stripping the `?url` query must not mask a genuinely
        // missing file — the bare path does not exist on disk, so it still fires.
        let source = "import file from './genuinely-missing.txt?url';";
        let diags = run_with_assets("src/app.ts", source, &[]);
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("genuinely-missing.txt"));
    }
}

#[cfg(test)]
mod fixture_dir_tests {
    use super::Check;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Run through the production applicability gate: build a real `FileCtx`
    /// from the importer path (so `is_relaxed_dir` reflects fixture dirs) and
    /// honour `applies_to_file` exactly as the engine does.
    fn run_gated(importer_rel: &str, source: &str) -> Vec<crate::diagnostic::Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        let importer = dir.path().join(importer_rel);
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, source).unwrap();
        let canon = fs::canonicalize(&importer).unwrap();
        let language = Language::from_path(&canon).unwrap();
        let source_file = SourceFile { path: canon.clone(), language };
        let project = ProjectCtx::load(&[&source_file], &Config::default());
        let file = FileCtx::build(&canon, source, language, &project);
        if !super::super::META.applies_to_file(&file) {
            return vec![];
        }
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &canon, &project, &file)
    }

    #[test]
    fn no_fp_for_eslint_config_fixture_issue_1650() {
        // expo reproducer: an eslint-config test fixture under
        // `__tests__/fixtures/` deliberately imports a non-existent module as
        // test input. The `fixtures/` segment marks a relaxed dir, so the rule
        // is skipped there.
        let source = "import e from './e';\nexport default e;";
        let diags = run_gated(
            "packages/eslint-config-expo/__tests__/fixtures/baseline/all-01.ts",
            source,
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn still_flags_missing_import_in_non_fixture_source_issue_1650() {
        // The same intentionally-missing import in ordinary (non-fixture)
        // source is a real broken path and must still fire — the exemption
        // stays scoped to relaxed dirs.
        let source = "import e from './e';\nexport default e;";
        let diags = run_gated("src/app/index.ts", source);
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("./e"));
    }
}

#[cfg(test)]
mod directory_import_tests {
    use super::Check;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Build a temp package tree (root `package.json` + the listed sibling
    /// files), run the rule on the importer, and return its diagnostics. Each
    /// `(rel, contents)` pair is written verbatim, so a test can stage a target
    /// directory's own `package.json` and entry file on disk.
    fn run_with_files(
        importer_rel: &str,
        source: &str,
        files: &[(&str, &str)],
    ) -> Vec<crate::diagnostic::Diagnostic> {
        let dir = TempDir::new().unwrap();
        for (rel, contents) in files {
            let p = dir.path().join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, contents).unwrap();
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
    fn no_fp_for_directory_import_via_main_issue_2350() {
        // koa reproducer: a test imports `../..`, the package root directory,
        // which Node resolves to `lib/application.js` via the `main` field.
        let source = "const Koa = require('../..');\n";
        let diags = run_with_files(
            "__tests__/context/state.test.js",
            source,
            &[
                (
                    "package.json",
                    r#"{"name":"koa","main":"lib/application.js"}"#,
                ),
                ("lib/application.js", "module.exports = class Koa {};\n"),
            ],
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn no_fp_for_directory_import_via_exports_issue_2350() {
        // The same root, resolved one level up (`../`) through the `exports`
        // `.` `require` condition.
        let source = "const required = require('../');\n";
        let diags = run_with_files(
            "__tests__/load.test.js",
            source,
            &[
                (
                    "package.json",
                    r#"{"name":"koa","exports":{".":{"require":"./lib/application.js"}}}"#,
                ),
                ("lib/application.js", "module.exports = class Koa {};\n"),
            ],
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn no_fp_for_directory_import_via_index_issue_2350() {
        // A relative directory import resolving to a conventional `index.js`
        // (no `package.json` entry needed) — already covered, kept as a guard.
        let source = "import { helper } from '../utils';\n";
        let diags = run_with_files(
            "src/app/main.js",
            source,
            &[("src/utils/index.js", "export const helper = 1;\n")],
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn still_flags_directory_import_with_missing_entry_issue_2350() {
        // Negative space: the target directory has a `package.json` whose `main`
        // points to a file that does NOT exist on disk, and there is no
        // `index.*` either — a genuinely broken import that must still fire.
        let source = "const broken = require('../..');\n";
        let diags = run_with_files(
            "__tests__/context/state.test.js",
            source,
            &[(
                "package.json",
                r#"{"name":"koa","main":"lib/application.js"}"#,
            )],
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("../.."));
    }

    #[test]
    fn still_flags_directory_import_without_entry_or_index_issue_2350() {
        // A directory exists but declares no entry and has no `index.*`: Node
        // cannot resolve it, so the import is a real error and must still fire.
        let source = "import x from '../utils';\n";
        let diags = run_with_files(
            "src/app/main.js",
            source,
            &[("src/utils/helper.js", "export const helper = 1;\n")],
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("../utils"));
    }
}
