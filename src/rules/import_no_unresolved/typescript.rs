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
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::run_ts_with_project_and_path;
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
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[1]);
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
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[1]);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_bare_specifier() {
        let (_dir, project, paths) = setup_project(&[("app.ts", "import React from 'react';")]);
        let source = "import React from 'react';";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_parent_relative_unresolved() {
        let (_dir, project, paths) =
            setup_project(&[("sub/app.ts", "import { x } from '../missing';")]);
        let source = "import { x } from '../missing';";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[0]);
        assert_eq!(diags.len(), 1);
    }
}
