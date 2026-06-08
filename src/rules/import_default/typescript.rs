//! import-default backend — verify default imports target modules that actually have a default export.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::{ExportKind, ImportKind};

crate::ast_check! { on ["program"] => |node, _source, ctx, diagnostics|
    let index = ctx.project.import_index();
    if index.is_empty() {
        return;
    }

    let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

    for imp in index.get_imports(&canon) {
        if imp.kind != ImportKind::Default {
            continue;
        }
        let Some(src) = &imp.source_path else {
            continue;
        };

        let exports = index.get_exports(src);

        // Bail if the source has `export * from '…'` — it might transitively re-export a default.
        if exports.iter().any(|e| e.kind == ExportKind::StarReExport) {
            continue;
        }

        let has_default = exports
            .iter()
            .any(|e| e.kind == ExportKind::Default || e.name == "default");

        if !has_default {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: imp.line,
                column: 1,
                rule_id: "import-default".into(),
                message: format!(
                    "No default export found in `{}`.",
                    imp.specifier
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
    fn flags_missing_default_export() {
        let (_dir, project, paths) = setup_project(&[
            ("utils.ts", "export const add = 1;"),
            ("app.ts", "import utils from './utils';"),
        ]);
        let source = "import utils from './utils';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[1], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("./utils"));
    }

    #[test]
    fn allows_default_export_present() {
        let (_dir, project, paths) = setup_project(&[
            ("utils.ts", "export default function add() {}"),
            ("app.ts", "import add from './utils';"),
        ]);
        let source = "import add from './utils';";
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
    fn skips_star_reexport() {
        let (_dir, project, paths) = setup_project(&[
            ("base.ts", "export default function base() {}"),
            ("utils.ts", "export * from './base';\nexport const add = 1;"),
            ("app.ts", "import foo from './utils';"),
        ]);
        let source = "import foo from './utils';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[2], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty());
    }
}
