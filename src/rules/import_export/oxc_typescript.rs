//! import-export OXC backend — flag duplicate export names within a single module.
//! This rule relies on the project-level import index, so it uses run_on_semantic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::ExportKind;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let index = ctx.project.import_index();
        if index.is_empty() {
            return Vec::new();
        }

        let canon = index.canonical(ctx.path);

        let exports = index.get_exports(&canon);
        let mut by_name: HashMap<(&str, bool), Vec<&crate::project::import_index::ExportedSymbol>> =
            HashMap::new();
        for exp in exports {
            if exp.kind == ExportKind::StarReExport {
                continue;
            }
            by_name.entry((exp.name.as_str(), exp.is_type_only)).or_default().push(exp);
        }

        let mut duplicates: Vec<(&str, usize)> = Vec::new();
        for ((name, _), occurrences) in &by_name {
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

        let mut diagnostics = Vec::new();
        for (name, line) in duplicates {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!("Duplicate export `{name}` in this module."),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::run_oxc_ts_with_project;
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
}
