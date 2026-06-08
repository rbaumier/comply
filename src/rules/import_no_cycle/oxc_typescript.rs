//! import-no-cycle OxcCheck backend.
//!
//! Detect circular import dependencies using the precomputed import index.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct Check;

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
    names.join(" \u{2192} ")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

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

        if let Some(cycle) = index.cycle_for(&canon) {
            let formatted = format_cycle(cycle, ctx.project.project_root.as_deref());
            vec![Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!("Circular import detected: {formatted}"),
                severity: Severity::Warning,
                span: None,
            }]
        } else {
            Vec::new()
        }
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
