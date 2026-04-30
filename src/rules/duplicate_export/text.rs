//! duplicate-export detection — aggregate every `ReExport` across the project
//! and flag symbol names that show up in two or more distinct barrel files.
//!
//! Skips:
//!   - `"default"` re-exports — barrels routinely re-export a default under
//!     different names; treating that as ambiguous would be noise.
//!   - `"*"` star re-exports — they don't carry a specific name to compare.
//!   - Symbols that appear in only one barrel — there is no duplication.
//!
//! Runs once per project, anchored on the lexicographically smallest indexed
//! path so that a single pass emits all diagnostics deterministically.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::ExportKind;
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const RULE_ID: &str = "duplicate-export";

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let index = ctx.project.import_index();

        // Once-per-project guard: only fire on the deterministic anchor.
        let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());
        let Some(anchor) = index.indexed_paths().min() else {
            return Vec::new();
        };
        if canon.as_path() != anchor {
            return Vec::new();
        }

        // symbol name -> list of (barrel file, line) where it is re-exported.
        let mut reexports: HashMap<String, Vec<(PathBuf, usize)>> = HashMap::new();
        for (path, exports) in index.iter_exports() {
            for export in exports {
                if !matches!(export.kind, ExportKind::ReExport) {
                    continue;
                }
                if export.name == "default" || export.name == "*" {
                    continue;
                }
                reexports
                    .entry(export.name.clone())
                    .or_default()
                    .push((path.to_path_buf(), export.line));
            }
        }

        let mut diagnostics = Vec::new();
        let mut names: Vec<&String> = reexports.keys().collect();
        names.sort();
        for name in names {
            let occurrences = &reexports[name];
            // Need at least two *distinct* barrel files re-exporting the name.
            let mut barrels: Vec<&Path> = occurrences.iter().map(|(p, _)| p.as_path()).collect();
            barrels.sort();
            barrels.dedup();
            if barrels.len() < 2 {
                continue;
            }
            // Anchor the diagnostic on the first occurrence (sorted by path)
            // for stable output. List every barrel in the message.
            let first = occurrences
                .iter()
                .min_by(|a, b| a.0.cmp(&b.0))
                .expect("at least one occurrence");
            let barrel_list = barrels
                .iter()
                .map(|p| format!("`{}`", p.display()))
                .collect::<Vec<_>>()
                .join(", ");
            diagnostics.push(Diagnostic {
                path: first.0.clone().into(),
                line: first.1,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!(
                    "symbol `{}` is re-exported by multiple barrels ({}). \
                     Pick a single canonical barrel to avoid ambiguous import paths.",
                    name, barrel_list
                ),
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
    use crate::rules::file_ctx::FileCtx;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            let lang = Language::from_path(&p).unwrap();
            source_files.push(SourceFile {
                path: p,
                language: lang,
            });
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx,
        };
        let diags = Check.check(&ctx);
        (dir, diags)
    }

    /// Pick the file the project's anchor rule will land on so the
    /// once-per-project guard fires inside `run_on_project`.
    fn anchor_rel<'a>(files: &'a [(&'a str, &'a str)]) -> &'a str {
        files
            .iter()
            .map(|(rel, _)| *rel)
            .min()
            .expect("at least one file")
    }

    #[test]
    fn flags_symbol_reexported_by_two_barrels() {
        let files: Vec<(&str, &str)> = vec![
            ("impl.ts", "export function compute() {}"),
            ("barrel1.ts", "export { compute } from './impl';"),
            ("barrel2.ts", "export { compute } from './impl';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(diags.len(), 1, "compute is re-exported by two barrels");
        assert_eq!(diags[0].rule_id, "duplicate-export");
        assert!(
            diags[0].message.contains("compute"),
            "message should name the duplicated symbol, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn allows_symbol_reexported_by_only_one_barrel() {
        let files: Vec<(&str, &str)> = vec![
            ("impl.ts", "export function compute() {}"),
            ("barrel.ts", "export { compute } from './impl';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(diags.is_empty(), "single barrel must not be flagged");
    }

    #[test]
    fn skips_default_reexports() {
        let files: Vec<(&str, &str)> = vec![
            ("impl.ts", "export default function compute() {}"),
            ("barrel1.ts", "export { default } from './impl';"),
            ("barrel2.ts", "export { default } from './impl';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "default re-exports must not be flagged, got: {:?}",
            diags
        );
    }
}
