//! import-export backend — flag duplicate export names within a single module.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::ExportKind;

crate::ast_check! { on ["program"] => |node, _source, ctx, diagnostics|
    let index = ctx.project.import_index();
    if index.is_empty() {
        return;
    }

    let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

    // Group exports by name. `StarReExport` has no specific name, skip it.
    let exports = index.get_exports(&canon);
    let mut by_name: HashMap<&str, Vec<&crate::project::import_index::ExportedSymbol>> =
        HashMap::new();
    for exp in exports {
        if exp.kind == ExportKind::StarReExport {
            continue;
        }
        by_name.entry(exp.name.as_str()).or_default().push(exp);
    }

    // For any name exported more than once, flag every occurrence after the first.
    let mut duplicates: Vec<(&str, usize)> = Vec::new();
    for (name, occurrences) in &by_name {
        if occurrences.len() < 2 {
            continue;
        }
        let mut lines: Vec<usize> = occurrences.iter().map(|e| e.line).collect();
        lines.sort_unstable();
        for line in lines.iter().skip(1) {
            duplicates.push((name, *line));
        }
    }

    duplicates.sort_by_key(|(_, line)| *line);

    for (name, line) in duplicates {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line,
            column: 1,
            rule_id: "import-export".into(),
            message: format!("Duplicate export `{name}` in this module."),
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
    fn flags_duplicate_export_name() {
        let source = "export const foo = 1;\nexport const foo = 2;\n";
        let (_dir, project, paths) = setup_project(&[("m.ts", source)]);
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[0]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
    }

    #[test]
    fn allows_unique_exports() {
        let source = "export const foo = 1;\nexport const bar = 2;\n";
        let (_dir, project, paths) = setup_project(&[("m.ts", source)]);
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_reexport_collision() {
        let source = "export { foo } from './a';\nexport { foo } from './b';\n";
        let (_dir, project, paths) = setup_project(&[
            ("a.ts", "export const foo = 1;\n"),
            ("b.ts", "export const foo = 2;\n"),
            ("m.ts", source),
        ]);
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[2]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
    }

    #[test]
    fn allows_default_and_named() {
        let source = "export default 1;\nexport const foo = 2;\n";
        let (_dir, project, paths) = setup_project(&[("m.ts", source)]);
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_star_reexport_with_named() {
        let source = "export * from './a';\nexport const foo = 1;\n";
        let (_dir, project, paths) = setup_project(&[
            ("a.ts", "export const bar = 1;\n"),
            ("m.ts", source),
        ]);
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[1]);
        assert!(diags.is_empty());
    }
}
