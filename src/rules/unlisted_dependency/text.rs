//! unlisted-dependency detection — walk every bare specifier collected in
//! the cross-file import index and flag the ones missing from any section
//! of `package.json` (dependencies, devDependencies, peerDependencies,
//! optionalDependencies, engines).
//!
//! A specifier is considered declared when it appears in the project-root
//! package.json, in any workspace member's name, or in the nearest
//! package.json to one of its importers (the monorepo case, where a member
//! package declares the dependency in its own manifest).
//!
//! Skips:
//!   - tsconfig path aliases (e.g. `@/utils`, `~/lib`) — they resolve to
//!     local source via `compilerOptions.paths`, not to an npm package.
//!   - Node.js subpath imports (`#`-prefixed specifiers) — Node reserves the
//!     `#` prefix for internal aliases resolved via a package.json `imports`
//!     map or a framework's module resolver (Nuxt's `#app`/`#imports`), never
//!     for npm package names.
//!   - build-time virtual modules — Vite's `virtual:`/colon-namespaced
//!     specifiers and Docusaurus framework aliases (`@theme/`, `@docusaurus/`)
//!     via `is_virtual_module`, plus the Docusaurus `@site/` source-root alias.
//!     These are resolved by the bundler, never published as npm packages.
//!   - the bare `bun` specifier — the Bun runtime's own built-in module
//!     (`Bun.Server`, `BunFile`, `Serve`, …), injected by the runtime and not
//!     installable from npm, like `node`/`node:*`. Only the exact `bun` is
//!     exempt; `bunyan`, `bun-types` and other `bun`-prefixed packages still
//!     fire. The `bun:` protocol siblings are covered by `is_virtual_module`.
//!
//! The rule produces project-wide diagnostics, not per-file ones, so it
//! fires only on the first indexed path of the run. Every other invocation
//! returns an empty diagnostic list. Each unlisted package is anchored on
//! the first importer file at line 1 — the actionable fix is editing
//! `package.json`, not the import site.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::no_implicit_deps::{is_virtual_module, types_package_name};
use crate::rules::path_utils::is_scaffold_template_path;

const RULE_ID: &str = "unlisted-dependency";

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(pkg) = ctx.project.package_json.as_ref() else {
            return Vec::new();
        };

        let index = ctx.project.import_index();

        // Run once per project: only fire on the lexicographically smallest
        // indexed path (deterministic across runs). Every other file short-circuits.
        let canon = index.canonical(ctx.path);
        let Some(anchor) = index.min_indexed_path() else {
            return Vec::new();
        };
        if canon.as_path() != anchor {
            return Vec::new();
        }

        let alias_prefixes = ctx
            .project
            .tsconfig
            .as_ref()
            .map(|t| t.alias_prefixes())
            .unwrap_or_default();

        let workspace_names: rustc_hash::FxHashSet<String> =
            ctx.project.workspace_package_names().iter().cloned().collect();

        let mut diagnostics = Vec::new();
        for (spec, info) in index.bare_specifiers() {
            if matches_alias(spec, &alias_prefixes) {
                continue;
            }
            // Virtual modules: Vite's `virtual:` convention and custom
            // namespace separators (`vitest-custom-virtual:math`) are plugin-
            // provided, never npm packages — a `:` is invalid in an npm name.
            // Also covers Docusaurus theme/core aliases (`@theme/`, `@docusaurus/`).
            if is_virtual_module(spec) {
                continue;
            }
            // Docusaurus maps `@site/` to the site root at build time (webpack),
            // so `@site/src/...` resolves to local project source, not an npm
            // package. `@site` is not a publishable npm name.
            if is_docusaurus_site_alias(spec) {
                continue;
            }
            // Bun runtime built-in: the bare `bun` specifier is the Bun
            // runtime's own module (`Bun.Server`, `BunFile`, `Serve`, …),
            // injected by the runtime and not installable from npm — analogous
            // to `node`/`node:*`. The `bun:` protocol siblings (`bun:test`,
            // `bun:sqlite`) are already covered by `is_virtual_module`'s
            // colon-scheme handling; the colonless bare `bun` is not, so it is
            // exempted explicitly. Only the exact `bun` matches — `bunyan`,
            // `bun-types` and other `bun`-prefixed npm packages still fire.
            if spec == "bun" {
                continue;
            }
            if pkg.has_dep_or_engine(spec) {
                continue;
            }
            // Node.js subpath imports: Node reserves the `#` prefix exclusively
            // for internal aliases — resolved either via a package.json `imports`
            // map or by a framework's module resolver (Nuxt's `#app`/`#imports`).
            // A `#`-specifier is therefore never an npm package name, so it can
            // never be an unlisted npm dependency. Mirrors `no-implicit-deps`,
            // which skips every `#`-specifier via `is_subpath_import`.
            if spec.starts_with('#') {
                continue;
            }
            if workspace_names.contains(spec) {
                continue;
            }
            // Scaffold-template content: when every importer of this specifier
            // lives in a `template/`/`templates/`/`scaffold/`/`boilerplate/`
            // directory, it is source a generator CLI (create-t3-app,
            // create-react-app) copies into the generated project. Such imports
            // describe the generated app's dependency graph, not the CLI's own —
            // the CLI's package.json never lists them. A specifier also imported
            // from a non-template file is still flagged (that importer is real
            // project code), so the guard requires ALL importers to be template
            // files before skipping.
            if !info.importers.is_empty()
                && info.importers.iter().all(|imp| is_scaffold_template_path(imp))
            {
                continue;
            }
            // DefinitelyTyped case: a type-only import of `X` is satisfied by
            // the `@types/X` package, whose runtime counterpart `X` may not
            // exist as a dependency (the value never reaches runtime). A value
            // import of `X` is NOT covered — `info.type_only` is false then.
            let types_pkg = info.type_only.then(|| types_package_name(spec));
            if let Some(types_pkg) = types_pkg.as_deref() {
                if pkg.has_dep_or_engine(types_pkg) {
                    continue;
                }
            }
            // Monorepo case: the dependency may be declared in the importing
            // package's own package.json rather than the project-root one (each
            // importer resolves its own manifest chain). When the importer's
            // nearest manifest is a private test/harness overlay, the chain also
            // includes the surrounding package, whose runtime deps the overlay's
            // files may import. The same chain resolves the `@types/X` provider.
            if info.importers.iter().any(|imp| {
                ctx.project.effective_package_jsons(imp).iter().any(|p| {
                    p.has_dep_or_engine(spec)
                        || types_pkg.as_deref().is_some_and(|t| p.has_dep_or_engine(t))
                })
            }) {
                continue;
            }
            // Anchor the diagnostic on the lexicographically-smallest importer
            // when available; fall back to the file the rule was invoked on.
            // `importers` is built in `import_index`'s `HashMap` iteration order
            // (randomized per process), so `.first()` would pick a different
            // carrier file run-to-run for a package imported from many sites.
            // Selecting the `min` path makes the carrier deterministic without
            // changing which packages are flagged.
            let anchor = info
                .importers
                .iter()
                .min()
                .cloned()
                .unwrap_or_else(|| ctx.path.to_path_buf());
            diagnostics.push(Diagnostic {
                path: anchor.into(),
                line: 1,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!(
                    "Import `{spec}` references an npm package not declared in package.json. \
                     Add it to dependencies or devDependencies."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

/// True if `spec` is a Docusaurus `@site/` alias, which the bundler maps to
/// the project source root at build time. Such specifiers resolve to local
/// files, never to an npm package (`@site` is not a publishable name).
fn is_docusaurus_site_alias(spec: &str) -> bool {
    spec == "@site" || spec.starts_with("@site/")
}

/// True if `spec` matches any tsconfig alias prefix (exact or `prefix/...`).
fn matches_alias(spec: &str, alias_prefixes: &[String]) -> bool {
    alias_prefixes.iter().any(|p| {
        if p.is_empty() {
            return false;
        }
        if spec == p.as_str() {
            return true;
        }
        if let Some(rest) = spec.strip_prefix(p.as_str()) {
            return rest.starts_with('/');
        }
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Build a project, locate the first indexed path (the one the
    /// run-once guard will accept), then run the check on it.
    fn run_on_project(
        files: &[(&str, &str)],
        package_json: Option<&str>,
        tsconfig: Option<&str>,
    ) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        if let Some(body) = package_json {
            fs::write(dir.path().join("package.json"), body).unwrap();
        }
        if let Some(body) = tsconfig {
            fs::write(dir.path().join("tsconfig.json"), body).unwrap();
        }
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            let lang = Language::from_path(&p).unwrap();
            source_files.push(SourceFile {
                path: p,
                language: lang,
            });
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = project
            .import_index()
            .indexed_paths()
            .min()
            .expect("at least one indexed file")
            .to_path_buf();
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx, lang: crate::files::Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        (dir, diags)
    }

    #[test]
    fn flags_unlisted_package() {
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import axios from 'axios';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert_eq!(diags.len(), 1, "axios should be flagged: {diags:?}");
        assert_eq!(diags[0].rule_id, "unlisted-dependency");
        assert!(
            diags[0].message.contains("axios"),
            "message should name the package, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn anchors_multi_site_dependency_on_lexicographically_first_importer() {
        // Regression for #1542 — when one unlisted package is imported from
        // multiple files, the diagnostic was anchored on `importers.first()`,
        // whose value came from `import_index`'s randomized `HashMap` iteration
        // order, so the carrier file churned run-to-run. The anchor must be the
        // lexicographically-smallest importer, stable across runs. Repeating the
        // selection must always yield the same `axios.ts` anchor.
        let files: Vec<(&str, &str)> = vec![
            ("z.ts", "import axios from 'axios';"),
            ("m.ts", "import axios from 'axios';"),
            ("axios.ts", "import axios from 'axios';"),
        ];
        // Repeat the full selection: every run must anchor on `axios.ts`. The
        // tempdir prefix differs per run but is a shared prefix, so the
        // lexicographic min is the same file every time.
        for _ in 0..8 {
            let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
            assert_eq!(diags.len(), 1, "axios should be flagged once: {diags:?}");
            let path = diags[0].path.to_string_lossy().to_string();
            assert!(
                path.ends_with("axios.ts"),
                "anchor must be the lexicographically-first importer (axios.ts), got: {path}"
            );
        }
    }

    #[test]
    fn allows_cloudflare_runtime_protocol_import() {
        // Regression for #2061 — `cloudflare:` is a Cloudflare Workers runtime
        // protocol namespace (like `node:`), not an npm package, so imports from
        // it must never be flagged even when nothing is declared.
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import { WorkerEntrypoint } from 'cloudflare:workers';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert!(
            diags.is_empty(),
            "`cloudflare:workers` is a runtime built-in, not an npm package: {diags:?}"
        );
    }

    #[test]
    fn allows_virtual_module_specifiers() {
        // Regression for #1975 — Vite virtual modules: the `virtual:` prefix
        // convention and custom namespace separators (`vitest-custom-virtual:math`)
        // are plugin-provided, never npm packages (a `:` is invalid in an npm
        // name), so they must not be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "a.ts",
                "import virtualFile1 from 'virtual:vitest-custom-virtual-file-1';\n\
                 import * as virtualMath from 'vitest-custom-virtual:math';",
            ),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert!(
            diags.is_empty(),
            "virtual module specifiers must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn allows_astro_virtual_module_protocol() {
        // Regression for #1933 — Astro virtual modules (`astro:content`,
        // `astro:transitions/client`) carry the `astro:` scheme, recognized as
        // a plugin-provided virtual namespace by the generic colon-scheme rule
        // (#1975) — a `:` is invalid in an npm name. They must not be flagged
        // even though `astro` is not a declared dependency.
        let files: Vec<(&str, &str)> = vec![
            (
                "a.ts",
                "import { defineCollection, z } from 'astro:content';\n\
                 import { navigate } from 'astro:transitions/client';",
            ),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert!(
            diags.is_empty(),
            "astro virtual module specifiers must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn allows_bun_runtime_protocol_imports() {
        // Regression for #1936 — Bun runtime built-ins (`bun:test`, `bun:sqlite`,
        // `bun:ffi`, `bun:jsc`) carry the `bun:` scheme, recognized as a virtual
        // namespace by the generic colon-scheme rule (#1975) — a `:` is invalid
        // in an npm name. They must not be flagged even though `bun`/`bun:test`
        // is not a declared dependency.
        let files: Vec<(&str, &str)> = vec![
            (
                "a.ts",
                "import { describe, test, expect } from 'bun:test';\n\
                 import { Database } from 'bun:sqlite';",
            ),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert!(
            diags.is_empty(),
            "bun: runtime protocol imports must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn allows_bare_bun_runtime_builtin_imports() {
        // Regression for #2098 — the bare `bun` specifier is the Bun runtime's
        // own built-in module (`Bun.Server`, `BunFile`, `Serve`, …), injected by
        // the runtime, not installable from npm (analogous to `node`/`node:*`).
        // Sibling to #1936, which exempted the `bun:` protocol specifiers. The
        // exact bare `bun` must be exempt even though it is not a declared dep,
        // for both value and type-only imports.
        let files: Vec<(&str, &str)> = vec![
            (
                "a.ts",
                "import { Serve } from 'bun';\n\
                 import type { Server } from 'bun';",
            ),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert!(
            diags.is_empty(),
            "bare `bun` is a Bun runtime built-in, not an npm package: {diags:?}"
        );
    }

    #[test]
    fn flags_bun_prefixed_npm_packages() {
        // Negative-space guard for #2098 — only the EXACT bare `bun` is exempt.
        // `bunyan` and `bun-types` are real installable npm packages that merely
        // start with `bun`, so an undeclared import of either must still fire.
        let files: Vec<(&str, &str)> = vec![
            (
                "a.ts",
                "import x from 'bunyan';\n\
                 import y from 'bun-types';",
            ),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert_eq!(
            diags.len(),
            2,
            "`bunyan` and `bun-types` are real npm packages and must still fire: {diags:?}"
        );
        assert!(diags.iter().any(|d| d.message.contains("bunyan")));
        assert!(diags.iter().any(|d| d.message.contains("bun-types")));
    }

    #[test]
    fn allows_url_imports() {
        // Regression for #1904 — `https://`/`http://` URL imports (CDN / browser
        // ESM) are resolved by the runtime, not npm. `extract_package_name`
        // would otherwise split the URL on `/` and yield the bogus package
        // `https:`, flagged as unlisted. They must never be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "custom-worker.js",
                "import 'https://unpkg.com/@typescript/vfs@1.3.0/dist/vfs.globals.js';\n\
                 import x from 'http://example.com/mod.js';",
            ),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert!(
            diags.is_empty(),
            "URL imports must not be flagged as unlisted dependencies: {diags:?}"
        );
    }

    #[test]
    fn allows_listed_dependency() {
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import _ from 'lodash';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(
            &files,
            Some(r#"{ "dependencies": { "lodash": "^4.0.0" } }"#),
            None,
        );
        assert!(diags.is_empty(), "lodash is declared: {diags:?}");
    }

    #[test]
    fn allows_dev_dependency() {
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import { test } from 'vitest';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(
            &files,
            Some(r#"{ "devDependencies": { "vitest": "^1.0.0" } }"#),
            None,
        );
        assert!(diags.is_empty(), "vitest is in devDependencies: {diags:?}");
    }

    #[test]
    fn allows_type_only_import_satisfied_by_types_package() {
        // Regression for #2059 — `import type { Root } from 'hast'` where only
        // `@types/hast` is declared (DefinitelyTyped). The runtime package
        // `hast` need not be a dependency; the types come from `@types/hast`.
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import type { Root } from 'hast';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(
            &files,
            Some(r#"{ "devDependencies": { "@types/hast": "*" } }"#),
            None,
        );
        assert!(
            diags.is_empty(),
            "type-only import of `hast` is satisfied by `@types/hast`: {diags:?}"
        );
    }

    #[test]
    fn allows_type_only_import_satisfied_by_scoped_types_package() {
        // Scoped mapping: `@foo/bar` resolves types from `@types/foo__bar`.
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import type { T } from '@foo/bar';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(
            &files,
            Some(r#"{ "devDependencies": { "@types/foo__bar": "*" } }"#),
            None,
        );
        assert!(
            diags.is_empty(),
            "type-only import of `@foo/bar` is satisfied by `@types/foo__bar`: {diags:?}"
        );
    }

    #[test]
    fn flags_value_import_when_only_types_package_declared() {
        // A runtime (value) import of `X` is NOT satisfied by `@types/X` alone —
        // the value must exist at runtime. Only `import type` is exempted.
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import { thing } from 'hast';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(
            &files,
            Some(r#"{ "devDependencies": { "@types/hast": "*" } }"#),
            None,
        );
        assert_eq!(
            diags.len(),
            1,
            "value import of `hast` is not covered by `@types/hast`: {diags:?}"
        );
        assert!(diags[0].message.contains("hast"));
    }

    #[test]
    fn flags_type_only_import_with_no_runtime_or_types_package() {
        // True positive: a type-only import where neither `X` nor `@types/X`
        // is declared still fires.
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import type { Root } from 'hast';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "devDependencies": {} }"#), None);
        assert_eq!(diags.len(), 1, "`hast` is unlisted everywhere: {diags:?}");
        assert!(diags[0].message.contains("hast"));
    }

    #[test]
    fn allows_subpath_import_declared_in_package_json_imports() {
        // Regression for #2063 — `#`-prefixed Node.js subpath imports declared
        // in package.json `imports` are self-referencing internal aliases, not
        // npm packages, and must not be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "a.ts",
                "import { importPlugin as _importPlugin } from '#import-plugin';",
            ),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(
            &files,
            Some(
                r##"{ "imports": { "#import-plugin": { "default": "./dist/import-plugin-default.js" } } }"##,
            ),
            None,
        );
        assert!(
            diags.is_empty(),
            "`#import-plugin` is declared in package.json `imports`: {diags:?}"
        );
    }

    #[test]
    fn allows_wildcard_subpath_import() {
        // A `#prefix/*` wildcard key covers any subpath under it.
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import { db } from '#internal/db';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(
            &files,
            Some(r##"{ "imports": { "#internal/*": "./src/internal/*.js" } }"##),
            None,
        );
        assert!(
            diags.is_empty(),
            "`#internal/db` is covered by the `#internal/*` wildcard: {diags:?}"
        );
    }

    #[test]
    fn allows_nuxt_framework_subpath_imports() {
        // Regression for #1723 — Nuxt runtime files import from `#app` and
        // `#imports`, framework-resolved virtual modules that are not declared
        // in any package.json `imports` map. Node reserves the `#` prefix for
        // internal aliases, so a `#`-specifier is never an npm package name and
        // must not be flagged as an unlisted dependency, declared or not.
        let files: Vec<(&str, &str)> = vec![
            (
                "a.ts",
                "import { useNuxtApp } from '#app';\n\
                 import { definePayloadReducer } from '#imports';",
            ),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert!(
            diags.is_empty(),
            "Nuxt `#app`/`#imports` are framework subpath imports, not npm packages: {diags:?}"
        );
    }

    #[test]
    fn flags_undeclared_bare_package() {
        // Negative-space guard for #1723 — the unconditional `#`-skip must not
        // leak into bare package names: a genuinely unlisted npm package
        // (`lodash`, no leading `#`) absent from package.json still fires.
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import _ from 'lodash';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert_eq!(diags.len(), 1, "`lodash` is unlisted: {diags:?}");
        assert!(diags[0].message.contains("lodash"));
    }

    #[test]
    fn allows_docusaurus_virtual_subpath_modules() {
        // Regression for #1689 — Docusaurus resolves `@docusaurus/*` and
        // `@theme/*` to virtual modules via `@docusaurus/core`, and maps
        // `@site/*` to the project source root at build time. None of these is
        // an npm package, so they must not be flagged as unlisted dependencies.
        let files: Vec<(&str, &str)> = vec![
            (
                "documentation/src/theme/DocItem/Layout/index.tsx",
                "import Link from '@docusaurus/Link';\n\
                 import { useHistory } from '@docusaurus/router';\n\
                 import useDocusaurusContext from '@docusaurus/useDocusaurusContext';\n\
                 import Layout from '@theme/Layout';\n\
                 import Tabs from '@theme/Tabs';\n\
                 import Hero from '@site/src/components/Hero';",
            ),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert!(
            diags.is_empty(),
            "Docusaurus `@docusaurus/*`, `@theme/*`, `@site/*` are framework \
             virtual modules, not npm packages: {diags:?}"
        );
    }

    #[test]
    fn flags_unlisted_package_alongside_docusaurus_aliases() {
        // Negative-space guard for #1689 — the Docusaurus exemptions must not
        // suppress a genuinely unlisted bare package imported in the same file.
        let files: Vec<(&str, &str)> = vec![
            (
                "a.tsx",
                "import Layout from '@theme/Layout';\n\
                 import { thing } from 'some-unlisted-pkg';",
            ),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert_eq!(
            diags.len(),
            1,
            "`some-unlisted-pkg` is a real bare import and must still fire: {diags:?}"
        );
        assert!(diags[0].message.contains("some-unlisted-pkg"));
    }

    #[test]
    fn allows_tsconfig_path_alias() {
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import { greet } from '@/utils';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(
            &files,
            Some(r#"{ "dependencies": {} }"#),
            Some(r#"{ "compilerOptions": { "paths": { "@/*": ["./*"] } } }"#),
        );
        assert!(
            diags.is_empty(),
            "tsconfig alias `@/*` should suppress the diagnostic: {diags:?}"
        );
    }

    #[test]
    fn allows_dep_declared_in_importer_package_json() {
        // Regression for #1400 — pnpm monorepo where `@pnpm/network.auth-header`
        // is declared in the importing package's package.json (a member), not in
        // the project-root package.json. The rule must consult the nearest
        // package.json to the importer, not only the root.
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","dependencies":{}}"#,
        )
        .unwrap();
        let member = dir.path().join("registry-access").join("commands");
        fs::create_dir_all(&member).unwrap();
        fs::write(
            member.join("package.json"),
            r#"{"name":"@pnpm/registry-access.commands","dependencies":{"@pnpm/network.auth-header":"workspace:*"}}"#,
        )
        .unwrap();

        let importer = member.join("src").join("unpublish.ts");
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(
            &importer,
            "import { getCredentialsByURI } from '@pnpm/network.auth-header';",
        )
        .unwrap();
        let root_file = dir.path().join("a.ts");
        fs::write(&root_file, "export const x = 1;").unwrap();

        let source_files = [
            SourceFile {
                path: importer.clone(),
                language: Language::TypeScript,
            },
            SourceFile {
                path: root_file,
                language: Language::TypeScript,
            },
        ];
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = project
            .import_index()
            .indexed_paths()
            .min()
            .expect("at least one indexed file")
            .to_path_buf();
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx,
            lang: crate::files::Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        assert!(
            diags.is_empty(),
            "`@pnpm/network.auth-header` is declared in the importer's package.json: {diags:?}"
        );
    }

    #[test]
    fn flags_dep_missing_from_importer_package_json() {
        // True-positive guard: a genuinely unlisted import in a member package
        // (absent from both the member and the root package.json) still fires.
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","dependencies":{}}"#,
        )
        .unwrap();
        let member = dir.path().join("registry-access").join("commands");
        fs::create_dir_all(&member).unwrap();
        fs::write(
            member.join("package.json"),
            r#"{"name":"@pnpm/registry-access.commands","dependencies":{"@pnpm/network.auth-header":"workspace:*"}}"#,
        )
        .unwrap();

        let importer = member.join("src").join("unpublish.ts");
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, "import axios from 'axios';").unwrap();
        let root_file = dir.path().join("a.ts");
        fs::write(&root_file, "export const x = 1;").unwrap();

        let source_files = [
            SourceFile {
                path: importer.clone(),
                language: Language::TypeScript,
            },
            SourceFile {
                path: root_file,
                language: Language::TypeScript,
            },
        ];
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = project
            .import_index()
            .indexed_paths()
            .min()
            .expect("at least one indexed file")
            .to_path_buf();
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx,
            lang: crate::files::Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        assert_eq!(diags.len(), 1, "axios is unlisted everywhere: {diags:?}");
        assert!(diags[0].message.contains("axios"));
    }

    #[test]
    fn allows_scaffold_template_only_import_issue_1381() {
        // Regression #1381: a scaffold CLI (create-t3-app, create-react-app)
        // keeps a `template/` directory of source files copied into the generated
        // project. `react` is imported only from a template file, so it describes
        // the generated app's deps, not the CLI's — the CLI's package.json never
        // lists it, and it must not be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "cli/template/extras/src/app/_components/post.tsx",
                "import { useState } from 'react';",
            ),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert!(
            diags.is_empty(),
            "an import only from a scaffold template dir must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn flags_import_from_both_template_and_src_issue_1381() {
        // Negative-space guard for #1381: a specifier imported from BOTH a
        // template file AND a real `src/` file must still fire — the real
        // importer is genuine project code missing the dependency.
        let files: Vec<(&str, &str)> = vec![
            (
                "cli/template/extras/post.tsx",
                "import { useState } from 'react';",
            ),
            ("src/app.tsx", "import { useEffect } from 'react';"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert_eq!(
            diags.len(),
            1,
            "a specifier also imported from non-template src/ must still fire: {diags:?}"
        );
        assert!(diags[0].message.contains("react"));
    }

    #[test]
    fn flags_import_in_template_substring_dir_issue_1381() {
        // Negative-space guard for #1381: a `templated/` substring directory is
        // ordinary source (segment match, not substring), so a missing dep fires.
        let files: Vec<(&str, &str)> = vec![
            ("src/templated/index.ts", "import { useState } from 'react';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, Some(r#"{ "dependencies": {} }"#), None);
        assert_eq!(
            diags.len(),
            1,
            "a `templated/` substring dir is ordinary source and must still fire: {diags:?}"
        );
        assert!(diags[0].message.contains("react"));
    }

    /// Build a 3-level layout (root with `workspaces`, an intermediate parent
    /// package, a nested package whose manifest is `nested_manifest`) where the
    /// nested file imports `import_spec`, then run the check. The parent package
    /// declares `vscode-languageserver-protocol`; the root declares nothing
    /// relevant. Used by the #2080 overlay tests.
    fn run_nested_overlay(nested_manifest: &str, import_spec: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"astro-monorepo","private":true,"workspaces":["packages/*"]}"#,
        )
        .unwrap();
        let pkg = dir.path().join("packages").join("language-server");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(
            pkg.join("package.json"),
            r#"{"name":"@astrojs/language-server","dependencies":{"vscode-languageserver-protocol":"^3"}}"#,
        )
        .unwrap();
        let nested = pkg.join("test");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("package.json"), nested_manifest).unwrap();
        let importer = nested.join("server.ts");
        fs::write(&importer, format!("import x from '{import_spec}';")).unwrap();
        let root_file = dir.path().join("a.ts");
        fs::write(&root_file, "export const x = 1;").unwrap();

        let source_files = [
            SourceFile {
                path: importer,
                language: Language::TypeScript,
            },
            SourceFile {
                path: root_file,
                language: Language::TypeScript,
            },
        ];
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = project
            .import_index()
            .indexed_paths()
            .min()
            .expect("at least one indexed file")
            .to_path_buf();
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx,
            lang: crate::files::Language::TypeScript,
        };
        Check.check(&ctx)
    }

    const PRIVATE_OVERLAY: &str =
        r#"{"name":"language-server-tests","private":true,"dependencies":{"astro":"workspace:*","svelte":"^5"}}"#;

    #[test]
    fn allows_parent_dep_imported_from_private_overlay_issue_2080() {
        // Regression #2080 — a `test/package.json` overlay (`private:true`, no
        // `workspaces`) declaring only test-extras. A test file importing the
        // parent package's declared runtime dep must not be flagged: the overlay
        // belongs to the surrounding package, whose deps it inherits.
        let diags = run_nested_overlay(PRIVATE_OVERLAY, "vscode-languageserver-protocol");
        assert!(diags.is_empty(), "parent dep from overlay must not fire: {diags:?}");
    }

    #[test]
    fn flags_dep_in_neither_overlay_nor_parent_issue_2080() {
        // Negative space for #2080: a dep declared by NEITHER the overlay nor the
        // parent still fires — the union only adds the parent's real deps.
        let diags = run_nested_overlay(PRIVATE_OVERLAY, "totally-undeclared-pkg");
        assert_eq!(diags.len(), 1, "undeclared dep must still fire: {diags:?}");
        assert!(diags[0].message.contains("totally-undeclared-pkg"));
    }

    #[test]
    fn flags_parent_dep_from_non_private_nested_issue_2080() {
        // Negative space for #2080: a non-private nested package is a real
        // standalone package — its files do NOT inherit parent deps, so the
        // parent-only dep still fires.
        let diags = run_nested_overlay(
            r#"{"name":"sub","dependencies":{"svelte":"^5"}}"#,
            "vscode-languageserver-protocol",
        );
        assert_eq!(diags.len(), 1, "non-private nested must not inherit: {diags:?}");
        assert!(diags[0].message.contains("vscode-languageserver-protocol"));
    }

    #[test]
    fn flags_parent_dep_from_private_workspace_root_issue_2080() {
        // Negative space for #2080: a private nested manifest that ALSO declares
        // `workspaces` is a workspace root, not an overlay — it does not walk up,
        // so the parent-only dep still fires.
        let diags = run_nested_overlay(
            r#"{"name":"sub","private":true,"workspaces":["pkgs/*"],"dependencies":{"svelte":"^5"}}"#,
            "vscode-languageserver-protocol",
        );
        assert_eq!(diags.len(), 1, "private workspace root must not inherit: {diags:?}");
        assert!(diags[0].message.contains("vscode-languageserver-protocol"));
    }

    #[test]
    fn allows_workspace_package() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        let foo = dir.path().join("packages").join("foo");
        fs::create_dir_all(&foo).unwrap();
        fs::write(foo.join("package.json"), r#"{"name":"@scope/foo"}"#).unwrap();

        // Create source file at the project root that imports the workspace package.
        let a_path = dir.path().join("a.ts");
        fs::write(&a_path, "import { greet } from '@scope/foo';").unwrap();
        let b_path = dir.path().join("b.ts");
        fs::write(&b_path, "export const x = 1;").unwrap();

        let source_files = [
            SourceFile {
                path: a_path.clone(),
                language: Language::TypeScript,
            },
            SourceFile {
                path: b_path,
                language: Language::TypeScript,
            },
        ];
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = project
            .import_index()
            .indexed_paths()
            .min()
            .expect("at least one indexed file")
            .to_path_buf();
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx, lang: crate::files::Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        assert!(
            diags.is_empty(),
            "workspace package `@scope/foo` should be recognized: {diags:?}"
        );
    }
}
