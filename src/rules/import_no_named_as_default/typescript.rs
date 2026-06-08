//! import-no-named-as-default backend — flag `import foo from './m'` when the
//! source module also exposes `foo` as a named export (likely a user mistake).

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::{ExportKind, ImportKind};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

crate::ast_check! { on ["program"] => |node, _source, ctx, diagnostics|
    let index = ctx.project.import_index();
    if index.is_empty() {
        return;
    }

    let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

    // Cache named exports per source path. `None` means "skip this source"
    // (it has `export * from '…'` so we can't enumerate names reliably).
    let mut named_by_source: HashMap<PathBuf, Option<HashSet<String>>> = HashMap::new();

    for imp in index.get_imports(&canon) {
        if imp.kind != ImportKind::Default {
            continue;
        }
        let Some(src) = &imp.source_path else {
            continue;
        };

        let named = named_by_source.entry(src.clone()).or_insert_with(|| {
            let exports = index.get_exports(src);
            if exports.iter().any(|e| e.kind == ExportKind::StarReExport) {
                return None;
            }
            Some(
                exports
                    .iter()
                    .filter(|e| e.kind != ExportKind::Default)
                    .map(|e| e.name.clone())
                    .collect(),
            )
        });

        let Some(named) = named else {
            continue;
        };

        if named.contains(&imp.local_name) {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: imp.line,
                column: 1,
                rule_id: "import-no-named-as-default".into(),
                message: format!(
                    "`{}` is a named export of `{}` — did you mean `import {{ {} }} from '{}'`?",
                    imp.local_name, imp.specifier, imp.local_name, imp.specifier
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
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
    fn flags_default_import_matching_named_export() {
        let (_dir, project, paths) = setup_project(&[
            ("utils.ts", "export const foo = 1;\nexport default 42;"),
            ("app.ts", "import foo from './utils';"),
        ]);
        let source = "import foo from './utils';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[1], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
    }

    #[test]
    fn allows_default_import_not_matching() {
        let (_dir, project, paths) = setup_project(&[
            ("utils.ts", "export const foo = 1;\nexport default 42;"),
            ("app.ts", "import bar from './utils';"),
        ]);
        let source = "import bar from './utils';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[1], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_bare_specifier() {
        let (_dir, project, paths) =
            setup_project(&[("app.ts", "import React from 'react';\nReact;")]);
        let source = "import React from 'react';\nReact;";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[0], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_no_named_exports() {
        let (_dir, project, paths) = setup_project(&[
            ("utils.ts", "export default 42;"),
            ("app.ts", "import utils from './utils';"),
        ]);
        let source = "import utils from './utils';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[1], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_source_with_star_reexport() {
        let (_dir, project, paths) = setup_project(&[
            ("base.ts", "export const foo = 1;"),
            ("utils.ts", "export * from './base';\nexport default 42;"),
            ("app.ts", "import foo from './utils';"),
        ]);
        let source = "import foo from './utils';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[2], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_named_import() {
        let (_dir, project, paths) = setup_project(&[
            ("utils.ts", "export const foo = 1;\nexport default 42;"),
            ("app.ts", "import { foo } from './utils';"),
        ]);
        let source = "import { foo } from './utils';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[1], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty());
    }
}
