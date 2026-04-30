//! import-named backend — verify every named import resolves to a real named export.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::{ExportKind, ImportKind};

crate::ast_check! { on ["program"] => |node, _source, ctx, diagnostics|
    let index = ctx.project.import_index();
    if index.is_empty() {
        return;
    }

    let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

    // Cache per-source export names. A source with `export * from '…'` is
    // skipped entirely — we can't enumerate transitive exports. `None` in the
    // cache means "bail, don't verify imports from this source".
    let mut exports_cache: HashMap<PathBuf, Option<HashSet<String>>> = HashMap::new();

    for imp in index.get_imports(&canon) {
        if imp.kind != ImportKind::Named {
            continue;
        }
        let Some(src) = &imp.source_path else {
            continue;
        };

        let entry = exports_cache.entry(src.clone()).or_insert_with(|| {
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
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: imp.line,
                column: 1,
                rule_id: "import-named".into(),
                message: format!(
                    "`{}` is not exported by `{}`.",
                    imp.imported_name, imp.specifier
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::run_ts_with_project_and_path;
    use std::fs;
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
    fn flags_nonexistent_named_import() {
        let (_dir, project, paths) = setup_project(&[
            (
                "utils.ts",
                "export const add = 1;\nexport const subtract = 2;",
            ),
            ("app.ts", "import { multiply } from './utils';"),
        ]);
        let source = "import { multiply } from './utils';";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[1]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("multiply"));
    }

    #[test]
    fn allows_existing_named_import() {
        let (_dir, project, paths) = setup_project(&[
            ("utils.ts", "export const add = 1;"),
            ("app.ts", "import { add } from './utils';"),
        ]);
        let source = "import { add } from './utils';";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[1]);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_star_reexport_source() {
        let (_dir, project, paths) = setup_project(&[
            ("base.ts", "export const x = 1;"),
            ("utils.ts", "export * from './base';\nexport const add = 1;"),
            ("app.ts", "import { anything } from './utils';"),
        ]);
        let source = "import { anything } from './utils';";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[2]);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_bare_specifier() {
        let (_dir, project, paths) =
            setup_project(&[("app.ts", "import { useState } from 'react';\nuseState();")]);
        let source = "import { useState } from 'react';\nuseState();";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_reexported_name() {
        let (_dir, project, paths) = setup_project(&[
            ("base.ts", "export const foo = 1;"),
            ("utils.ts", "export { foo } from './base';"),
            ("app.ts", "import { foo } from './utils';"),
        ]);
        let source = "import { foo } from './utils';";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[2]);
        assert!(diags.is_empty());
    }
}
