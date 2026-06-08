//! import-no-unresolved OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::collections::HashSet;
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
        let index = ctx.project.import_index();
        if index.is_empty() {
            return Vec::new();
        }

        let canon = index.canonical(ctx.path);
        let mut seen: HashSet<(String, usize)> = HashSet::new();
        let mut diagnostics = Vec::new();

        for imp in index.get_imports(&canon) {
            let is_relative = imp.specifier.starts_with("./") || imp.specifier.starts_with("../");
            if !is_relative {
                continue;
            }
            if imp.source_path.is_some() {
                continue;
            }
            // Skip gitignored build-time generated files (e.g. TanStack
            // Router's `routeTree.gen.ts`): often absent at lint time, always
            // present at build/dev time.
            if is_generated_specifier(&imp.specifier) {
                continue;
            }
            if !seen.insert((imp.specifier.clone(), imp.line)) {
                continue;
            }

            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: imp.line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Unable to resolve import path `{}` — file does not exist.",
                    imp.specifier
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

/// True for specifiers pointing at a build-time generated file whose final
/// segment ends in `.gen` (e.g. `./routeTree.gen`) or carries a `.gen.`
/// extension stem (e.g. `./routeTree.gen.ts`). Such files are gitignored and
/// often absent at lint time, yet always present at build/dev time.
fn is_generated_specifier(spec: &str) -> bool {
    let last = spec.rsplit('/').next().unwrap_or(spec);
    last.ends_with(".gen") || last.contains(".gen.")
}

#[cfg(test)]
mod oxc_tests {
    use super::is_generated_specifier;

    #[test]
    fn detects_generated_specifiers_issue_487() {
        assert!(is_generated_specifier("./routeTree.gen"));
        assert!(is_generated_specifier("./routeTree.gen.ts"));
        assert!(is_generated_specifier("../app/routeTree.gen"));
        assert!(!is_generated_specifier("./routeTree"));
        assert!(!is_generated_specifier("./generated"));
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


    #[test]
    fn allows_import_of_existing_dts_file() {
        let dir = TempDir::new().unwrap();
        let dts_path = dir.path().join("index.d.ts");
        fs::write(&dts_path, "export type Schema = {};").unwrap();
        let ts_path = dir.path().join("test-d/schema.ts");
        fs::create_dir_all(ts_path.parent().unwrap()).unwrap();
        fs::write(&ts_path, "import type { Schema } from '../index.d.ts';").unwrap();
        let lang = Language::from_path(&ts_path).unwrap();
        let source_files = vec![SourceFile {
            path: ts_path.clone(),
            language: lang,
        }];
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon_ts = fs::canonicalize(&ts_path).unwrap();
        let source = "import type { Schema } from '../index.d.ts';";
        let diags = run_oxc_ts_with_project(source, &Check, &project);
        assert!(diags.is_empty(), "unexpected FP: {diags:?}");
    }
}
