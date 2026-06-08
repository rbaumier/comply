//! OxcCheck backend for import-named.
//!
//! This rule uses the project import index, not AST — same as the
//! tree-sitter version. We use `run_on_semantic` with an empty
//! `interested_kinds` since the real work is index-based.

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

        for imp in index.get_imports(&canon) {
            if imp.kind != ImportKind::Named {
                continue;
            }
            let Some(src) = &imp.source_path else {
                continue;
            };

            let entry = exports_cache.entry(src.clone()).or_insert_with(|| {
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

    use crate::rules::test_helpers::run_oxc_ts_with_project;


    fn setup_project(files: &[(&str, &str)]) -> (TempDir, ProjectCtx, Vec<PathBuf>) {
        let dir = TempDir::new().unwrap();
        let mut source_files = Vec::new();
        let mut paths = Vec::new();

        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            let lang = Language::from_path(&p).unwrap();
            source_files.push(SourceFile {
                path: p.clone(),
                language: lang,
            });
            paths.push(fs::canonicalize(&p).unwrap());
        }

        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        (dir, project, paths)
    }
}
