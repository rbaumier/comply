//! OxcCheck backend for import-named.
//!
//! This rule uses the project import index, not AST — same as the
//! tree-sitter version. We use `run_on_semantic` with an empty
//! `interested_kinds` since the real work is index-based.
//!
//! Imports from `.d.ts` declaration files are not verified: declaration files
//! are excluded from the scan set, so the index has no export data for them.
//!
//! For imports that resolve to a runtime source file, the export set is widened
//! with the type-only names declared in the file's companion `.d.ts` (the
//! same-stem sibling and the nearest package's `"types"`/`"typings"` target).
//! TypeScript resolves named type imports against those declarations, so a name
//! present only there must not be reported as missing.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::{ExportKind, ImportKind};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let index = ctx.project.import_index();
        if index.is_empty() {
            return diagnostics;
        }

        let canon = index.canonical(ctx.path);
        let mut exports_cache: HashMap<PathBuf, Option<HashSet<String>>> = HashMap::new();

        // Names of every local workspace member (pnpm/yarn/npm monorepo). A
        // cross-package import addresses a sibling by its scoped package name
        // (`@myorg/other`); resolving that name to the member's public exports
        // crosses the package boundary (build artifacts, `exports`/conditions,
        // re-export indirection) and is not reliably indexed. Absence of a
        // resolvable export set is not absence of the export — skip these.
        let workspace_names: HashSet<&str> = ctx
            .project
            .workspace_package_names()
            .iter()
            .map(String::as_str)
            .collect();

        for imp in index.get_imports(&canon) {
            if imp.kind != ImportKind::Named {
                continue;
            }
            if workspace_names.contains(root_package_name(&imp.specifier)) {
                continue;
            }
            let Some(src) = &imp.source_path else {
                continue;
            };

            let entry = exports_cache.entry(src.clone()).or_insert_with(|| {
                // `.d.ts` declaration files are valid import targets but are
                // intentionally excluded from the scan set, so the index has no
                // export data for them. Without a reliable export set we cannot
                // verify named imports against a declaration file — skip rather
                // than emit false positives (type-fest et al. export only types
                // from `.d.ts`).
                if is_declaration_file(src) {
                    return None;
                }
                // Framework entry points (route trees, generated manifests) and
                // @generated files may have synthesised exports not tracked by
                // the index — skip verification so tests importing from them
                // don't produce false positives.
                if crate::rules::path_utils::is_framework_entry_point(src, ctx.project) {
                    return None;
                }
                if is_generated_file(src) {
                    return None;
                }
                let exports = index.get_exports(src);
                // `export *` re-exports and a non-enumerable CJS
                // `module.exports = <expr>` both leave the export set
                // unknowable from this file alone — skip rather than report a
                // name as missing.
                if exports
                    .iter()
                    .any(|e| matches!(e.kind, ExportKind::StarReExport | ExportKind::OpaqueCjs))
                {
                    return None;
                }
                let mut names: HashSet<String> =
                    exports.iter().map(|e| e.name.clone()).collect();
                // A package's type-only named exports (`export type Foo`) live in
                // a companion `.d.ts` declaration, not the runtime JS the
                // specifier resolves to. Fold those names in so a valid
                // type-only import isn't reported as missing.
                match companion_declaration_exports(src) {
                    CompanionExports::Names(decl_names) => names.extend(decl_names),
                    // The companion declaration has an unenumerable `export *` —
                    // skip rather than risk a false positive.
                    CompanionExports::Unenumerable => return None,
                    CompanionExports::None => {}
                }
                Some(names)
            });

            let Some(export_names) = entry.as_ref() else {
                continue;
            };

            if !export_names.contains(&imp.imported_name) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: imp.line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{}` is not exported by `{}`.",
                        imp.imported_name, imp.specifier
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

/// Root package name of a bare specifier: `@scope/pkg/deep` → `@scope/pkg`,
/// `lodash/fp` → `lodash`. Only used to match against workspace member names,
/// which are always bare scoped/unscoped package names.
fn root_package_name(specifier: &str) -> &str {
    if specifier.starts_with('@') {
        // `@scope/pkg/...` — keep the first two slash-separated segments.
        let end = specifier
            .match_indices('/')
            .nth(1)
            .map(|(idx, _)| idx)
            .unwrap_or(specifier.len());
        return &specifier[..end];
    }
    specifier.split('/').next().unwrap_or(specifier)
}

fn is_declaration_file(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| {
            n.ends_with(".d.ts")
                || n.ends_with(".d.mts")
                || n.ends_with(".d.cts")
                || n.ends_with(".d.tsx")
        })
}

fn is_generated_file(path: &std::path::Path) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    let mut end = content.len().min(2048);
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    content[..end].contains("@generated")
}

/// Outcome of looking for the type-only export names a non-declaration source
/// file's companion `.d.ts` contributes.
enum CompanionExports {
    /// No companion declaration file was found.
    None,
    /// A companion exists but re-exports via `export *`, so its full export set
    /// cannot be enumerated from a single-file parse. Callers should skip
    /// verification to avoid false positives.
    Unenumerable,
    /// The companion's named exports.
    Names(HashSet<String>),
}

/// Names exported by the declaration file(s) accompanying a runtime source file.
/// TypeScript resolves a named import against these declarations even though the
/// specifier resolves to the runtime JS. Two companion locations are checked:
///
/// - the sibling `.d.ts` of the same stem (`fastify.js` → `fastify.d.ts`),
///   covering local `.js` imports backed by a declaration file;
/// - the declaration pointed to by the package's `"types"`/`"typings"` field,
///   covering bare imports of an npm package whose types live in a `.d.ts`.
fn companion_declaration_exports(src: &std::path::Path) -> CompanionExports {
    let mut names: HashSet<String> = HashSet::new();
    let mut found = false;

    for decl in companion_declaration_paths(src) {
        match crate::project::import_index::declaration_file_exports(&decl) {
            Some(decl_names) => {
                found = true;
                names.extend(decl_names);
            }
            None => return CompanionExports::Unenumerable,
        }
    }

    if found {
        CompanionExports::Names(names)
    } else {
        CompanionExports::None
    }
}

/// Existing companion declaration files for a runtime source file: the
/// same-stem sibling and the nearest package's `"types"`/`"typings"` target.
fn companion_declaration_paths(src: &std::path::Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(sibling) = sibling_declaration_path(src)
        && sibling.is_file()
    {
        paths.push(sibling);
    }

    if let Some(types) = package_types_declaration_path(src)
        && types.is_file()
        && !paths.contains(&types)
    {
        paths.push(types);
    }

    paths
}

/// Sibling declaration path for a runtime file: `foo.js` → `foo.d.ts`,
/// `foo.mjs` → `foo.d.mts`, `foo.cjs` → `foo.d.cts`. Returns `None` for files
/// without a recognised runtime extension.
fn sibling_declaration_path(src: &std::path::Path) -> Option<PathBuf> {
    let ext = src.extension().and_then(|e| e.to_str())?;
    let decl_ext = match ext {
        "js" | "jsx" | "ts" | "tsx" => "d.ts",
        "mjs" | "mts" => "d.mts",
        "cjs" | "cts" => "d.cts",
        _ => return None,
    };
    Some(src.with_extension(decl_ext))
}

/// Declaration file pointed to by the `"types"` or `"typings"` field of the
/// nearest `package.json` at or above `src`. Stops at the first `package.json`
/// found walking up.
fn package_types_declaration_path(src: &std::path::Path) -> Option<PathBuf> {
    let mut dir = src.parent();
    while let Some(d) = dir {
        let manifest = d.join("package.json");
        if manifest.is_file() {
            let raw = std::fs::read_to_string(&manifest).ok()?;
            let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
            let types = json
                .get("types")
                .or_else(|| json.get("typings"))
                .and_then(serde_json::Value::as_str)?;
            return Some(d.join(types));
        }
        dir = d.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::backend::CheckCtx;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> (TempDir, Vec<Diagnostic>) {
        run_on_project_with_pkg(None, files, target_rel)
    }

    fn run_on_project_with_pkg(
        package_json: Option<&str>,
        files: &[(&str, &str)],
        target_rel: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        if let Some(pkg) = package_json {
            fs::write(dir.path().join("package.json"), pkg).unwrap();
        }
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            if let Some(lang) = Language::from_path(&p) {
                source_files.push(SourceFile { path: p, language: lang });
            }
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let canon = fs::canonicalize(&target_path).unwrap();

        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, &source, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&canon, &source, &project);
        let diags = Check.run_on_semantic(&semantic, &ctx);
        (dir, diags)
    }

    #[test]
    fn no_fp_on_test_file_importing_from_generated_route_tree() {
        // Regression for #382 — test file imports `UsersUserIdRoute` from the
        // generated TanStack Router route tree. The route tree carries a
        // `@generated` header and may not export individual route objects in
        // older generator versions; import-named must not flag the import.
        let pkg = r#"{ "dependencies": { "@tanstack/react-router": "1.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/generated/routeTree.ts",
                "// @generated by @tanstack/router-cli\nexport const routeTree = {};",
            ),
            (
                "src/app/routes/users.$userId.tsx",
                "export const Route = createLazyFileRoute('/users/$userId')({});",
            ),
            (
                "src/app/routes/-users.$userId.test.tsx",
                "import { UsersUserIdRoute } from '../../generated/routeTree';\nconst r = UsersUserIdRoute;",
            ),
        ];
        let (_dir, diags) = run_on_project_with_pkg(
            Some(pkg),
            &files,
            "src/app/routes/-users.$userId.test.tsx",
        );
        assert!(
            diags.is_empty(),
            "test importing from @generated route tree must not be flagged by import-named: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_test_file_importing_from_framework_entry_route_tree() {
        // Regression for #382 — same pattern but route tree is at the
        // canonical `routeTree.gen.ts` path (framework entry file). Even if
        // the generated tree doesn't list `UsersUserIdRoute` explicitly,
        // import-named must stay silent.
        let pkg = r#"{ "dependencies": { "@tanstack/react-router": "1.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/routeTree.gen.ts",
                "export const routeTree = {};",
            ),
            (
                "src/app/routes/users.$userId.tsx",
                "export const Route = createLazyFileRoute('/users/$userId')({});",
            ),
            (
                "src/app/routes/-users.$userId.test.tsx",
                "import { UsersUserIdRoute } from '../../routeTree.gen';\nconst r = UsersUserIdRoute;",
            ),
        ];
        let (_dir, diags) = run_on_project_with_pkg(
            Some(pkg),
            &files,
            "src/app/routes/-users.$userId.test.tsx",
        );
        assert!(
            diags.is_empty(),
            "test importing from routeTree.gen.ts (framework entry) must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_ambient_declare_function_export_issue_2030() {
        // Regression for #2030 — `shared.ts` exports `test` via an ambient
        // `export declare function` and `SimpleCaseData` via `export declare
        // const`; importing both from it must not be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "shared.ts",
                "export declare function test(name: string, body: () => void): void;\n\
                 export declare const SimpleCaseData: number;\n",
            ),
            (
                "useLazyQuery.ts",
                "import { test, SimpleCaseData } from './shared.js';\n\
                 test('x', () => {});\n\
                 const d = SimpleCaseData;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "useLazyQuery.ts");
        assert!(
            diags.is_empty(),
            "import of ambient `export declare function`/`const` must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn still_flags_missing_name_from_ambient_module_issue_2030() {
        // True positive preserved: a name the ambient module does not export is
        // still reported.
        let files: Vec<(&str, &str)> = vec![
            ("shared.ts", "export declare function test(name: string): void;\n"),
            (
                "consumer.ts",
                "import { test, missing } from './shared.js';\ntest('x');\nconst m = missing;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "consumer.ts");
        assert_eq!(diags.len(), 1, "absent name must still be flagged: {diags:?}");
        assert!(diags[0].message.contains("missing"));
    }

    #[test]
    fn no_fp_on_export_namespace_import_issue_1774() {
        // Regression for #1774 — sst pattern: `link.ts` declares `Link` as a
        // TypeScript `export namespace Link {}` and `rpc.ts` declares `rpc` the
        // same way. A named import of either must not be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "components/link.ts",
                "export namespace Link {\n\
                 export interface Definition { properties: Record<string, unknown>; }\n\
                 export function reset() {}\n\
                 }\n\
                 export interface Linkable { urn: string; }\n",
            ),
            (
                "components/rpc/rpc.ts",
                "export namespace rpc {\n\
                 export class MethodNotFoundError extends Error {}\n\
                 export async function call(method: string) { return method; }\n\
                 }\n",
            ),
            (
                "auto/run.ts",
                "import { Link } from '../components/link';\n\
                 import { rpc } from '../components/rpc/rpc.js';\n\
                 Link.reset();\n\
                 rpc.call('x');\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "auto/run.ts");
        assert!(
            diags.is_empty(),
            "named import of an `export namespace` must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_star_as_namespace_reexport_issue_1218() {
        // Regression for #1218 — effect-ts barrel: `index.ts` re-exports each
        // submodule under a namespace via `export * as Effect from './Effect.js'`.
        // A named import of those namespaces must not be flagged.
        let files: Vec<(&str, &str)> = vec![
            ("src/Effect.ts", "export const succeed = 1;\n"),
            ("src/Option.ts", "export const some = 1;\n"),
            (
                "src/index.ts",
                "export * as Effect from './Effect.js';\nexport * as Option from './Option.js';\n",
            ),
            (
                "src/app.ts",
                "import { Effect, Option } from './index.js';\nconst a = Effect;\nconst b = Option;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/app.ts");
        assert!(
            diags.is_empty(),
            "named import of an `export * as Name` namespace re-export must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn still_flags_missing_name_from_star_as_namespace_reexport_issue_1218() {
        // True positive preserved: a name the barrel does not re-export (only
        // `export * as Effect` exists) is still reported.
        let files: Vec<(&str, &str)> = vec![
            ("src/Effect.ts", "export const succeed = 1;\n"),
            ("src/index.ts", "export * as Effect from './Effect.js';\n"),
            (
                "src/app.ts",
                "import { Effect, Missing } from './index.js';\nconst a = Effect;\nconst b = Missing;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/app.ts");
        assert_eq!(diags.len(), 1, "absent name must still be flagged: {diags:?}");
        assert!(diags[0].message.contains("Missing"));
    }

    #[test]
    fn still_skips_verification_on_bare_star_reexport() {
        // Guard: a bare `export * from './x.js'` (no `as`) stays a wildcard
        // StarReExport — the barrel's full export set is unenumerable from a
        // single-file parse, so import-named skips verification entirely. A name
        // not directly declared in the barrel must NOT be flagged.
        let files: Vec<(&str, &str)> = vec![
            ("src/inner.ts", "export const fromInner = 1;\n"),
            ("src/index.ts", "export * from './inner.js';\n"),
            (
                "src/app.ts",
                "import { fromInner } from './index.js';\nconst a = fromInner;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/app.ts");
        assert!(
            diags.is_empty(),
            "bare `export *` re-export must keep skipping verification: {diags:?}"
        );
    }

    #[test]
    fn still_flags_bad_import_from_regular_file() {
        // Guard: import-named still fires on a misspelled import from a
        // normal (non-generated, non-framework) source file.
        let files: Vec<(&str, &str)> = vec![
            ("src/utils.ts", "export const add = 1;"),
            (
                "src/app.test.ts",
                "import { multiply } from './utils';\nconst x = multiply;",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/app.test.ts");
        assert_eq!(diags.len(), 1, "bad import must still be flagged");
        assert!(diags[0].message.contains("multiply"));
    }

    #[test]
    fn no_fp_on_named_type_import_from_d_ts_issue_1052() {
        // type-fest pattern: `import type { And } from '../source/and.d.ts'`.
        // .d.ts files are excluded from the scan set, so the index has no
        // export data — import-named must not flag them.
        let files: Vec<(&str, &str)> = vec![
            ("source/and.d.ts", "export type And<A, B> = [A, B];\n"),
            (
                "test-d/and.ts",
                "import type { And } from '../source/and.d.ts';\nconst x: And<number, string> = [1, 'a'];\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "test-d/and.ts");
        assert!(diags.is_empty(), "import from .d.ts must not be flagged: {diags:?}");
    }

    // Build an installed pnpm workspace: node_modules/@effect/sql-mssql is a
    // symlink to packages/sql-mssql, mirroring how oxc_resolver resolves a
    // sibling workspace package by walking node_modules. Returns the diags for
    // the given target source.
    fn run_workspace(target_src: &str) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        let member = dir.path().join("packages/sql-mssql");
        fs::create_dir_all(member.join("src")).unwrap();
        fs::create_dir_all(member.join("test")).unwrap();
        fs::write(
            member.join("package.json"),
            r#"{"name":"@effect/sql-mssql","main":"./src/index.ts"}"#,
        )
        .unwrap();
        // Barrel re-exports each submodule under a namespace, the shape the
        // effect monorepo uses. The cross-package export set is not reliably
        // indexed across the package boundary.
        fs::write(
            member.join("src/index.ts"),
            "export * as MssqlClient from './MssqlClient.js';\nexport * as MssqlMigrator from './MssqlMigrator.js';\n",
        )
        .unwrap();
        fs::write(member.join("src/MssqlClient.ts"), "export const make = 1;\n").unwrap();
        fs::write(member.join("src/MssqlMigrator.ts"), "export const run = 1;\n").unwrap();
        let target = member.join("test/Client.test.ts");
        fs::write(&target, target_src).unwrap();

        let nm = dir.path().join("node_modules/@effect");
        fs::create_dir_all(&nm).unwrap();
        std::os::unix::fs::symlink(&member, nm.join("sql-mssql")).unwrap();

        // Root-level source so common_ancestor is the monorepo root, letting
        // detect_project_root find the root package.json with `workspaces`.
        fs::write(dir.path().join("root.ts"), "export const root = 1;\n").unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for rel in [
            "root.ts",
            "packages/sql-mssql/src/index.ts",
            "packages/sql-mssql/src/MssqlClient.ts",
            "packages/sql-mssql/src/MssqlMigrator.ts",
            "packages/sql-mssql/test/Client.test.ts",
        ] {
            let p = dir.path().join(rel);
            if let Some(lang) = Language::from_path(&p) {
                source_files.push(SourceFile { path: p, language: lang });
            }
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&target).unwrap();
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, target_src, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&canon, target_src, &project);
        let diags = Check.run_on_semantic(&semantic, &ctx);
        (dir, diags)
    }

    #[test]
    fn root_package_name_extracts_scope_and_subpaths() {
        assert_eq!(root_package_name("@effect/sql-mssql"), "@effect/sql-mssql");
        assert_eq!(root_package_name("@effect/sql-mssql/Migrator"), "@effect/sql-mssql");
        assert_eq!(root_package_name("lodash"), "lodash");
        assert_eq!(root_package_name("lodash/fp"), "lodash");
    }

    #[test]
    fn no_fp_on_cross_workspace_named_import_issue_1423() {
        // Regression for #1423 — a named import from a sibling pnpm workspace
        // package (`@effect/sql-mssql`) whose public exports cannot be enumerated
        // across the package boundary must not be flagged as "not exported".
        let (_dir, diags) = run_workspace(
            "import { MssqlClient } from '@effect/sql-mssql';\nconst x = MssqlClient;\n",
        );
        assert!(
            diags.is_empty(),
            "cross-workspace named import must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_cross_workspace_subpath_named_import_issue_1423() {
        // Subpath import of a workspace package (`@effect/sql-mssql/Migrator`)
        // resolves to the same workspace member — still must not be flagged.
        let (_dir, diags) = run_workspace(
            "import { Procedure } from '@effect/sql-mssql/Migrator';\nconst x = Procedure;\n",
        );
        assert!(
            diags.is_empty(),
            "cross-workspace subpath named import must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_named_type_import_from_d_ts_reexport_issue_1052() {
        // type-only re-export through a .d.ts barrel (type-fest index.d.ts).
        let files: Vec<(&str, &str)> = vec![
            ("source/schema.d.ts", "export type Schema = { a: number };\n"),
            ("index.d.ts", "export type { Schema } from './source/schema.d.ts';\n"),
            (
                "test-d/schema.ts",
                "import type { Schema } from '../index.d.ts';\nconst y: Schema = { a: 1 };\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "test-d/schema.ts");
        assert!(diags.is_empty(), "import from .d.ts re-export must not be flagged: {diags:?}");
    }

    #[test]
    fn no_fp_on_named_type_import_from_sibling_dts_issue_1648() {
        // fastify pattern: types live in the companion `fastify.d.ts`, the
        // runtime `fastify.js` exports only values. A named type import from
        // the `.js` must resolve against the sibling declaration.
        let files: Vec<(&str, &str)> = vec![
            ("fastify.js", "export const fastify = 1;\nexport const errorCodes = 2;\n"),
            (
                "fastify.d.ts",
                "export type FastifyInstance = { x: number };\n\
                 export interface FastifyReply { y: number }\n",
            ),
            (
                "test/using.ts",
                "import { fastify, FastifyInstance, FastifyReply } from '../fastify.js';\n\
                 const a = fastify;\n\
                 const b: FastifyInstance = { x: 1 };\n\
                 const c: FastifyReply = { y: 2 };\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "test/using.ts");
        assert!(
            diags.is_empty(),
            "named type import backed by a sibling .d.ts must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn still_flags_missing_name_with_sibling_dts() {
        // True positive preserved: a name present in neither the runtime JS nor
        // the companion .d.ts is still reported.
        let files: Vec<(&str, &str)> = vec![
            ("fastify.js", "export const fastify = 1;\n"),
            ("fastify.d.ts", "export type FastifyInstance = { x: number };\n"),
            (
                "test/using.ts",
                "import { fastify, Nonexistent } from '../fastify.js';\n\
                 const a = fastify;\n\
                 type B = Nonexistent;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "test/using.ts");
        assert_eq!(diags.len(), 1, "absent name must still be flagged: {diags:?}");
        assert!(diags[0].message.contains("Nonexistent"));
    }

    // Build an installed npm package: node_modules/preact resolves a bare
    // `preact` specifier to its runtime `main` JS, while its type-only exports
    // live in the `.d.ts` pointed to by the `"types"` field of the package's
    // own package.json. Returns the diags for the given target source.
    fn run_with_package(
        package_json: &str,
        package_files: &[(&str, &str)],
        target_src: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"app","dependencies":{"preact":"10.0.0"}}"#,
        )
        .unwrap();

        let pkg = dir.path().join("node_modules/preact");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("package.json"), package_json).unwrap();
        for (rel, content) in package_files {
            let p = pkg.join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
        }

        let target = dir.path().join("src/router.tsx");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, target_src).unwrap();

        // The package's runtime JS must be indexed so its export set is known;
        // include the package files and the consumer in the input set.
        let mut rels: Vec<PathBuf> = vec![target.clone()];
        for (rel, _) in package_files {
            rels.push(pkg.join(rel));
        }
        let mut source_files: Vec<SourceFile> = Vec::new();
        for p in &rels {
            if let Some(lang) = Language::from_path(p) {
                source_files.push(SourceFile { path: p.clone(), language: lang });
            }
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let canon = fs::canonicalize(&target).unwrap();
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, target_src, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&canon, target_src, &project);
        let diags = Check.run_on_semantic(&semantic, &ctx);
        (dir, diags)
    }

    #[test]
    fn no_fp_on_named_type_import_from_package_types_dts_issue_1922() {
        // preact pattern: type-only named exports (`ComponentChild`, …) live in
        // the `.d.ts` pointed to by the package's `"types"` field; the runtime
        // `main` JS exports only values. A named import of those types from the
        // bare package specifier must not be flagged.
        let pkg = r#"{"name":"preact","main":"dist/preact.js","types":"src/index.d.ts"}"#;
        let package_files: Vec<(&str, &str)> = vec![
            (
                "dist/preact.js",
                "export const render = 1;\nexport const createContext = 2;\n",
            ),
            (
                "src/index.d.ts",
                "export type ComponentChild = string | number | null;\n\
                 export type FunctionalComponent<P> = (props: P) => ComponentChild;\n\
                 export type ComponentFactory<P> = FunctionalComponent<P>;\n",
            ),
        ];
        let target = "import { ComponentChild, FunctionalComponent, ComponentFactory, createContext } from 'preact';\n\
                      const a: ComponentChild = null;\n\
                      const b = createContext;\n";
        let (_dir, diags) = run_with_package(pkg, &package_files, target);
        assert!(
            diags.is_empty(),
            "type-only named imports backed by the package \"types\" .d.ts must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_cjs_module_exports_object_issue_3315() {
        // Regression for #3315 — trpc/trpc www pattern: `env.js` declares
        // `parseEnv` and exposes it via `module.exports = { parseEnv }`. A named
        // import of it from a `.ts` file must not be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/utils/env.js",
                "function parseEnv(input) { return input; }\nmodule.exports = { parseEnv };\n",
            ),
            (
                "src/useEnv.ts",
                "import { parseEnv } from './utils/env';\nconst x = parseEnv;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/useEnv.ts");
        assert!(
            diags.is_empty(),
            "named import from a CJS `module.exports = {{ … }}` must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_cjs_exports_property_issue_3315() {
        // `exports.foo = …` (the other enumerable CJS form) names `foo` as a
        // named export; importing it must not be flagged.
        let files: Vec<(&str, &str)> = vec![
            ("m.js", "exports.foo = 1;\n"),
            ("app.ts", "import { foo } from './m';\nconst x = foo;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "app.ts");
        assert!(
            diags.is_empty(),
            "named import from `exports.foo = …` must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_non_enumerable_cjs_module_exports_issue_3315() {
        // `module.exports = someFn` — the export set is not statically
        // enumerable, so import-named must skip rather than report any name as
        // missing (Part B opaque-CJS guard).
        let files: Vec<(&str, &str)> = vec![
            ("m.js", "function someFn() {}\nmodule.exports = someFn;\n"),
            ("app.ts", "import { anything } from './m';\nconst x = anything;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "app.ts");
        assert!(
            diags.is_empty(),
            "named import from a non-enumerable `module.exports = <expr>` must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn still_flags_missing_name_from_es_module_issue_3315() {
        // Guard: a genuine ES module with a real export still gets a bogus name
        // flagged — the opaque-CJS guard must be narrow.
        let files: Vec<(&str, &str)> = vec![
            ("m.ts", "export const a = 1;\n"),
            ("app.ts", "import { doesNotExist } from './m';\nconst x = doesNotExist;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "app.ts");
        assert_eq!(diags.len(), 1, "absent name in an ES module must still be flagged: {diags:?}");
        assert!(diags[0].message.contains("doesNotExist"));
    }

    #[test]
    fn still_flags_missing_name_from_cjs_object_exports_issue_3315() {
        // Guard: `module.exports = { parseEnv }` enumerates only `parseEnv`, so a
        // name genuinely absent from the object literal is still flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "env.js",
                "function parseEnv(input) { return input; }\nmodule.exports = { parseEnv };\n",
            ),
            ("app.ts", "import { notThere } from './env';\nconst x = notThere;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "app.ts");
        assert_eq!(diags.len(), 1, "absent name in an enumerable CJS object must still be flagged: {diags:?}");
        assert!(diags[0].message.contains("notThere"));
    }

    #[test]
    fn still_flags_missing_name_with_package_types_dts() {
        // True positive preserved: a name exported by neither the runtime JS nor
        // the package's `"types"` declaration is still reported.
        let pkg = r#"{"name":"preact","main":"dist/preact.js","types":"src/index.d.ts"}"#;
        let package_files: Vec<(&str, &str)> = vec![
            ("dist/preact.js", "export const render = 1;\n"),
            ("src/index.d.ts", "export type ComponentChild = string | null;\n"),
        ];
        let target = "import { render, Nope } from 'preact';\n\
                      const a = render;\n\
                      type B = Nope;\n";
        let (_dir, diags) = run_with_package(pkg, &package_files, target);
        assert_eq!(diags.len(), 1, "absent name must still be flagged: {diags:?}");
        assert!(diags[0].message.contains("Nope"));
    }
}
