//! import-no-named-as-default OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::import_index::{ExportKind, ImportKind};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let index = ctx.project.import_index();
        if index.is_empty() {
            return diagnostics;
        }

        let canon = index.canonical(ctx.path);

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
                let (_line, _column) =
                    byte_offset_to_line_col(ctx.source, 0);
                // Use the import's line directly — it comes from the import index.
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: imp.line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{}` is a named export of `{}` — did you mean `import {{ {} }} from '{}'`?",
                        imp.local_name, imp.specifier, imp.local_name, imp.specifier
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
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
