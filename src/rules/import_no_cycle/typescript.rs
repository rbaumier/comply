//! import-no-cycle backend — detect circular import dependencies.
//! Uses Tarjan SCC computed once in `ImportIndex::build()`; this rule just
//! looks up the precomputed cycle (if any) for the current file.

use crate::diagnostic::{Diagnostic, Severity};
use std::path::{Path, PathBuf};

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

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let index = ctx.project.import_index();
    if index.is_empty() {
        return;
    }

    let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

    if let Some(cycle) = index.cycle_for(&canon) {
        let formatted = format_cycle(cycle, ctx.project.project_root.as_deref());
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
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
    fn detects_simple_cycle() {
        let (_dir, project, paths) = setup_project(&[
            ("a.ts", "import { b } from './b';"),
            ("b.ts", "import { a } from './a';"),
        ]);

        let source = "import { b } from './b';";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[0], &project, crate::rules::file_ctx::default_static_file_ctx());
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
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[0], &project, crate::rules::file_ctx::default_static_file_ctx());
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
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[0], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_no_imports() {
        let (_dir, project, paths) = setup_project(&[("a.ts", "export const a = 1;")]);

        let source = "export const a = 1;";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &paths[0], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty());
    }
}
