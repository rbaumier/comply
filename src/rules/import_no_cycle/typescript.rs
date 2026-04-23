//! import-no-cycle backend — detect circular import dependencies.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn find_cycle(
    index: &crate::project::ImportIndex,
    start: &Path,
    current: &Path,
    visited: &mut HashSet<PathBuf>,
    path: &mut Vec<PathBuf>,
) -> Option<Vec<PathBuf>> {
    if visited.contains(current) {
        if current == start && !path.is_empty() {
            let mut cycle = path.clone();
            cycle.push(current.to_path_buf());
            return Some(cycle);
        }
        return None;
    }

    visited.insert(current.to_path_buf());
    path.push(current.to_path_buf());

    for imp in index.get_imports(current) {
        if let Some(source) = &imp.source_path {
            if source == start && !path.is_empty() {
                let mut cycle = path.clone();
                cycle.push(source.clone());
                return Some(cycle);
            }
            if let Some(cycle) = find_cycle(index, start, source, visited, path) {
                return Some(cycle);
            }
        }
    }

    path.pop();
    None
}

fn format_cycle(cycle: &[PathBuf], root: Option<&Path>) -> String {
    let names: Vec<&str> = cycle
        .iter()
        .map(|p| {
            if let Some(r) = root {
                p.strip_prefix(r)
                    .ok()
                    .and_then(|s| s.to_str())
                    .unwrap_or_else(|| p.file_name().and_then(|n| n.to_str()).unwrap_or("?"))
            } else {
                p.file_name().and_then(|n| n.to_str()).unwrap_or("?")
            }
        })
        .collect();
    names.join(" → ")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let index = ctx.project.import_index();
    if index.is_empty() {
        return;
    }

    let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

    let mut visited = HashSet::new();
    let mut path = Vec::new();

    if let Some(cycle) = find_cycle(index, &canon, &canon, &mut visited, &mut path) {
        let formatted = format_cycle(&cycle, ctx.project.project_root.as_deref());
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: 1,
            column: 1,
            rule_id: "import-no-cycle".into(),
            message: format!("Circular import detected: {formatted}"),
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
    fn detects_simple_cycle() {
        let (_dir, project, paths) = setup_project(&[
            ("a.ts", "import { b } from './b';"),
            ("b.ts", "import { a } from './a';"),
        ]);

        let source = "import { b } from './b';";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[0]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Circular import"));
    }

    #[test]
    fn detects_transitive_cycle() {
        let (_dir, project, paths) = setup_project(&[
            ("a.ts", "import { b } from './b';"),
            ("b.ts", "import { c } from './c';"),
            ("c.ts", "import { a } from './a';"),
        ]);

        let source = "import { b } from './b';";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[0]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Circular import"));
    }

    #[test]
    fn allows_no_cycle() {
        let (_dir, project, paths) = setup_project(&[
            ("a.ts", "import { b } from './b';"),
            ("b.ts", "import { c } from './c';"),
            ("c.ts", "export const c = 1;"),
        ]);

        let source = "import { b } from './b';";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_no_imports() {
        let (_dir, project, paths) = setup_project(&[("a.ts", "export const a = 1;")]);

        let source = "export const a = 1;";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }
}
