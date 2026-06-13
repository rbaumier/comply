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
//!   - Node.js subpath imports (`#`-prefixed specifiers) declared in the
//!     `imports` field of the root or an importer's nearest package.json —
//!     self-referencing aliases resolved to internal files, not npm packages.
//!
//! The rule produces project-wide diagnostics, not per-file ones, so it
//! fires only on the first indexed path of the run. Every other invocation
//! returns an empty diagnostic list. Each unlisted package is anchored on
//! the first importer file at line 1 — the actionable fix is editing
//! `package.json`, not the import site.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::no_implicit_deps::is_virtual_module;

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
            if is_virtual_module(spec) {
                continue;
            }
            if pkg.has_dep_or_engine(spec) {
                continue;
            }
            // Node.js subpath imports: a `#`-prefixed specifier is a
            // self-referencing alias declared in a package.json `imports` map,
            // resolved to an internal file at runtime — never an npm package.
            // Exempt it when the root or any importer's nearest manifest
            // declares it under `imports`.
            if spec.starts_with('#')
                && (pkg.declares_subpath_import(spec)
                    || info.importers.iter().any(|imp| {
                        ctx.project
                            .nearest_package_json(imp)
                            .is_some_and(|p| p.declares_subpath_import(spec))
                    }))
            {
                continue;
            }
            if workspace_names.contains(spec) {
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
            // package's own package.json rather than the project-root one
            // (each importer walks up to its nearest manifest). The same
            // nearest-manifest walk also resolves the `@types/X` provider.
            if info.importers.iter().any(|imp| {
                ctx.project.nearest_package_json(imp).is_some_and(|p| {
                    p.has_dep_or_engine(spec)
                        || types_pkg.as_deref().is_some_and(|t| p.has_dep_or_engine(t))
                })
            }) {
                continue;
            }
            // Anchor the diagnostic on the first importer when available;
            // fall back to the file the rule was invoked on.
            let anchor = info
                .importers
                .first()
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

/// DefinitelyTyped name for a runtime package: `foo` → `@types/foo`,
/// `@scope/bar` → `@types/scope__bar` (scope marker folded to `__`).
fn types_package_name(spec: &str) -> String {
    if let Some(scoped) = spec.strip_prefix('@') {
        return format!("@types/{}", scoped.replacen('/', "__", 1));
    }
    format!("@types/{spec}")
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
    fn flags_undeclared_subpath_import() {
        // Precision guard: a `#`-specifier with no matching key in `imports`
        // is not exempted by the subpath-import branch.
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import { x } from '#not-declared';"),
            ("b.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(
            &files,
            Some(r##"{ "imports": { "#other": "./src/other.js" } }"##),
            None,
        );
        assert_eq!(
            diags.len(),
            1,
            "`#not-declared` is absent from `imports`: {diags:?}"
        );
        assert!(diags[0].message.contains("#not-declared"));
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
