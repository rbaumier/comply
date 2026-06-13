//! OxcCheck backend for import-named.
//!
//! This rule uses the project import index, not AST — same as the
//! tree-sitter version. We use `run_on_semantic` with an empty
//! `interested_kinds` since the real work is index-based.
//!
//! Imports from `.d.ts` declaration files are not verified: declaration files
//! are excluded from the scan set, so the index has no export data for them.

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
                if exports.iter().any(|e| e.kind == ExportKind::StarReExport) {
                    return None;
                }
                Some(exports.iter().map(|e| e.name.clone()).collect())
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
}
