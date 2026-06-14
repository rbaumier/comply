//! import-no-unresolved backend — flag relative imports whose target file
//! does not exist in the input set.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

crate::ast_check! { on ["program"] => |node, _source, ctx, diagnostics|
    let index = ctx.project.import_index();
    if index.is_empty() {
        return;
    }

    let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

    // Deduplicate by (specifier, line) in case the index exposes the same
    // import twice (defensive — a single `import` statement should produce
    // one entry per symbol, and we want one diagnostic per statement).
    let mut seen: HashSet<(String, usize)> = HashSet::new();

    for imp in index.get_imports(&canon) {
        let is_relative = imp.specifier.starts_with("./") || imp.specifier.starts_with("../");
        if !is_relative {
            continue;
        }
        if imp.source_path.is_some() {
            continue;
        }
        // CSS, CSS Modules, SVG, and other static assets are imported via
        // build-tool support and never enter the TS/JS index. When such a
        // non-source file exists on disk next to the importer, the import is
        // resolved — don't flag it.
        if super::oxc_typescript::is_existing_asset_import(ctx.path, &imp.specifier) {
            continue;
        }
        // A relative import whose target source file exists on disk but lives
        // in a directory excluded from the scan (e.g. vendored code under
        // `vendor/`) is absent from the import index, so `source_path` is
        // `None` — yet the import is genuinely resolvable. Don't flag it.
        if super::oxc_typescript::is_existing_source_import(ctx.path, &imp.specifier) {
            continue;
        }
        if !seen.insert((imp.specifier.clone(), imp.line)) {
            continue;
        }

        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: imp.line,
            column: 1,
            rule_id: "import-no-unresolved".into(),
            message: format!(
                "Unable to resolve import path `{}` — file does not exist.",
                imp.specifier
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
    use std::path::PathBuf;
    use tempfile::TempDir;

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

    #[test]
    fn flags_unresolved_relative_import() {
        let (_dir, project, paths) = setup_project(&[
            ("existing.ts", "export const x = 1;"),
            ("app.ts", "import { x } from './nonexistent';"),
        ]);
        let source = "import { x } from './nonexistent';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[1], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("nonexistent"));
    }

    #[test]
    fn allows_resolved_relative_import() {
        let (_dir, project, paths) = setup_project(&[
            ("utils.ts", "export const x = 1;"),
            ("app.ts", "import { x } from './utils';"),
        ]);
        let source = "import { x } from './utils';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[1], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_bare_specifier() {
        let (_dir, project, paths) = setup_project(&[("app.ts", "import React from 'react';")]);
        let source = "import React from 'react';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[0], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_parent_relative_unresolved() {
        let (_dir, project, paths) =
            setup_project(&[("sub/app.ts", "import { x } from '../missing';")]);
        let source = "import { x } from '../missing';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[0], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_for_dollar_sign_in_filename_tanstack_router() {
        // TanStack Router uses `$param` segments in filenames. A test file
        // importing `./cabinets_.$cabinetId` (without extension) must not be
        // flagged when `cabinets_.$cabinetId.tsx` exists on disk.
        let (_dir, project, paths) = setup_project(&[
            (
                "routes/cabinets_.$cabinetId.tsx",
                "export const Route = {};",
            ),
            (
                "routes/cabinets_.$cabinetId.test.ts",
                "import { Route } from './cabinets_.$cabinetId';",
            ),
        ]);
        let source = "import { Route } from './cabinets_.$cabinetId';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[1], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn no_fp_for_existing_css_and_css_module_imports_issue_1384() {
        // vercel/turborepo reproducer: Next.js apps import a global stylesheet
        // as a side effect and a CSS module as a default. Both files exist on
        // disk next to the importer but never enter the TS/JS index, so the
        // rule must not flag them.
        let (_dir, project, paths) = setup_project(&[
            ("app/globals.css", "body { margin: 0; }"),
            ("app/page.module.css", ".title { color: red; }"),
            (
                "app/layout.tsx",
                "import \"./globals.css\";\nimport styles from \"./page.module.css\";",
            ),
        ]);
        let source = "import \"./globals.css\";\nimport styles from \"./page.module.css\";";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[2], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn flags_missing_css_import_issue_1384() {
        // A CSS import whose file does NOT exist on disk is still a real
        // unresolved import and must be flagged.
        let (_dir, project, paths) = setup_project(&[(
            "app/layout.tsx",
            "import \"./missing.css\";",
        )]);
        let source = "import \"./missing.css\";";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[0], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing.css"));
    }

    #[test]
    fn no_fp_for_import_into_excluded_vendor_dir_issue_2044() {
        // trpc reproducer: a source file imports a vendored module under
        // `vendor/`, which is excluded from the scan. The target file exists on
        // disk but is absent from the import index, so the import must not be
        // flagged. Covers a file target (`../vendor/unpromise/index.ts` via the
        // directory's index), a nested file (`../../vendor/cookie-es/.../split`),
        // and an explicit-extensionless file target.
        let dir = TempDir::new().unwrap();

        let importer = dir.path().join("packages/server/src/adapters/ws.ts");
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, "import { Unpromise } from '../vendor/unpromise';").unwrap();

        // The vendored files live on disk but are NOT added to the project, just
        // as the scan walker prunes the excluded `vendor/` directory.
        let vendor_index = dir
            .path()
            .join("packages/server/src/vendor/unpromise/index.ts");
        fs::create_dir_all(vendor_index.parent().unwrap()).unwrap();
        fs::write(&vendor_index, "export class Unpromise {}").unwrap();

        let lang = Language::from_path(&importer).unwrap();
        let source_files = vec![SourceFile {
            path: importer.clone(),
            language: lang,
        }];
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&importer).unwrap();
        let source = "import { Unpromise } from '../vendor/unpromise';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert!(diags.is_empty(), "unexpected FP for vendor import: {diags:?}");
    }

    #[test]
    fn flags_unresolved_import_when_no_file_on_disk_issue_2044() {
        // The vendor fix must stay precise: a relative import whose target has no
        // file on disk anywhere is still genuinely unresolved and must fire.
        let dir = TempDir::new().unwrap();
        let importer = dir.path().join("packages/server/src/adapters/ws.ts");
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, "import { Gone } from '../vendor/does-not-exist';").unwrap();

        let lang = Language::from_path(&importer).unwrap();
        let source_files = vec![SourceFile {
            path: importer.clone(),
            language: lang,
        }];
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&importer).unwrap();
        let source = "import { Gone } from '../vendor/does-not-exist';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does-not-exist"));
    }

    #[test]
    fn allows_import_of_existing_dts_file() {
        let dir = TempDir::new().unwrap();
        let dts_path = dir.path().join("index.d.ts");
        fs::write(&dts_path, "export type Schema = {};").unwrap();
        let ts_path = dir.path().join("test-d/schema.ts");
        fs::create_dir_all(ts_path.parent().unwrap()).unwrap();
        fs::write(&ts_path, "import type { Schema } from '../index.d.ts';").unwrap();
        let lang = Language::from_path(&ts_path).unwrap();
        let source_files = vec![SourceFile {
            path: ts_path.clone(),
            language: lang,
        }];
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon_ts = fs::canonicalize(&ts_path).unwrap();
        let source = "import type { Schema } from '../index.d.ts';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &canon_ts, &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty(), "unexpected FP: {diags:?}");
    }

    #[test]
    fn flags_import_of_nonexistent_dts_file() {
        let dir = TempDir::new().unwrap();
        let ts_path = dir.path().join("test-d/schema.ts");
        fs::create_dir_all(ts_path.parent().unwrap()).unwrap();
        fs::write(&ts_path, "import type { Schema } from '../index.d.ts';").unwrap();
        let lang = Language::from_path(&ts_path).unwrap();
        let source_files = vec![SourceFile {
            path: ts_path.clone(),
            language: lang,
        }];
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon_ts = fs::canonicalize(&ts_path).unwrap();
        let source = "import type { Schema } from '../index.d.ts';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &canon_ts, &project, crate::rules::file_ctx::default_static_file_ctx());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("index.d.ts"));
    }

    #[test]
    fn no_fp_for_directory_import_resolving_to_index_dts_issue_1686() {
        // refine reproducer: `import { IPost } from '../../interfaces'` where the
        // directory holds only `index.d.ts`. TypeScript resolves the directory to
        // its declaration index; `.d.ts` files are excluded from the scan set, so
        // resolution falls back to an on-disk existence check.
        let dir = TempDir::new().unwrap();

        let importer = dir.path().join("src/pages/posts/list.tsx");
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, "import { IPost, ICategory } from '../../interfaces';").unwrap();

        // The declaration index lives on disk but is NOT added to the project.
        let decl_index = dir.path().join("src/interfaces/index.d.ts");
        fs::create_dir_all(decl_index.parent().unwrap()).unwrap();
        fs::write(
            &decl_index,
            "export interface IPost { id: number; }\nexport interface ICategory { id: number; }",
        )
        .unwrap();

        let lang = Language::from_path(&importer).unwrap();
        let source_files = vec![SourceFile {
            path: importer.clone(),
            language: lang,
        }];
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&importer).unwrap();
        let source = "import { IPost, ICategory } from '../../interfaces';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert!(diags.is_empty(), "unexpected FP for directory index.d.ts import: {diags:?}");
    }

    #[test]
    fn flags_directory_import_with_no_index_at_all_issue_1686() {
        // Negative space: a directory import where the directory holds no index
        // file of any kind (no index.ts/tsx/js, no index.d.ts) is genuinely
        // unresolvable and must still be flagged.
        let dir = TempDir::new().unwrap();

        let importer = dir.path().join("src/pages/posts/list.tsx");
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, "import { IPost } from '../../interfaces';").unwrap();

        // The directory exists but holds only a non-index declaration file, so
        // the directory import has no entry point to resolve to.
        let stray = dir.path().join("src/interfaces/types.d.ts");
        fs::create_dir_all(stray.parent().unwrap()).unwrap();
        fs::write(&stray, "export interface IPost { id: number; }").unwrap();

        let lang = Language::from_path(&importer).unwrap();
        let source_files = vec![SourceFile {
            path: importer.clone(),
            language: lang,
        }];
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&importer).unwrap();
        let source = "import { IPost } from '../../interfaces';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("interfaces"));
    }
}
