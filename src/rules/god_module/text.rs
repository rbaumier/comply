//! god-module detection — cross-file check via `ProjectCtx::import_index()`.
//!
//! For each indexed TS/JS/TSX file, count how many other indexed files import
//! from it. If that count is both:
//!   - at least `min_importers` in absolute terms (defaults.toml: 10), AND
//!   - at least `threshold_percent` of the total indexed file count (30%),
//!     emit a diagnostic at line 1 of the offending module.
//!
//! The `min_importers` gate exists because in a project with 8 files every
//! shared helper would look like a god module by fraction alone — absolute
//! thresholding keeps the rule useful on realistic codebases only.
//!
//! Path handling: `ImportIndex` stores canonicalised absolute paths, while
//! `ctx.path` is whatever the user passed on the command line. We canonicalise
//! before looking up, and fall back to the raw path if canonicalize fails
//! (file deleted mid-run) — in that case the lookup misses silently.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const RULE_ID: &str = "god-module";

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let index = ctx.project.import_index();

        // Total indexed files = every file that made it through
        // `ImportIndex::build`. `iter_exports` enumerates exports per file
        // but the map contains an entry for every indexed TS/JS/TSX file
        // (exports vec may be empty), so the count is the denominator we want.
        let total_files = index.iter_exports().count();
        if total_files == 0 {
            // No cross-file index available (LSP / single-file run). The rule
            // has no signal to act on.
            return Vec::new();
        }

        let threshold_percent = ctx.config.threshold(RULE_ID, "threshold_percent", ctx.lang);
        let min_importers = ctx.config.threshold(RULE_ID, "min_importers", ctx.lang);

        let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());
        let importers = index.get_importers(&canon);
        let importer_count = importers.len();

        if importer_count < min_importers {
            return Vec::new();
        }

        // Integer math: fire when `importer_count / total_files > threshold / 100`.
        // Rearranged to avoid floats / rounding surprises:
        //   importer_count * 100 > threshold_percent * total_files
        if importer_count * 100 <= threshold_percent * total_files {
            return Vec::new();
        }

        // Percentage shown in the message is floor(importer_count * 100 / total).
        let percent = (importer_count * 100) / total_files;
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: RULE_ID.into(),
            message: format!(
                "imported by {importer_count}/{total_files} files ({percent}%). \
                 Consider splitting into smaller, focused modules."
            ),
            severity: Severity::Warning,
            span: None,
        }]
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
    use tempfile::TempDir;

    /// Build a project on disk with N files, all importing from `hub.ts`,
    /// plus `extra_files` untouched files to control the ratio.
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
        let file_ctx = FileCtx::empty();
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx, lang: crate::files::Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        (dir, diags)
    }

    #[test]
    fn flags_hub_imported_by_more_than_threshold() {
        // 1 hub + 12 importers = 13 files. Importers/total = 12/13 ~= 92%,
        // well above 30% and well above min_importers = 10.
        let mut files: Vec<(String, String)> = Vec::new();
        files.push(("hub.ts".to_string(), "export const x = 1;\n".to_string()));
        for i in 0..12 {
            files.push((
                format!("a{i}.ts"),
                "import { x } from './hub';\n".to_string(),
            ));
        }
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let (_dir, diags) = run_on_project(&borrowed, "hub.ts");
        assert_eq!(diags.len(), 1, "expected one god-module diagnostic");
        assert_eq!(diags[0].rule_id, "god-module");
    }

    #[test]
    fn allows_module_below_threshold_percent() {
        // 1 hub + 2 importers out of 20 files = 10%, below 30%.
        let mut files: Vec<(String, String)> = vec![
            ("hub.ts".into(), "export const x = 1;\n".into()),
            ("a0.ts".into(), "import { x } from './hub';\n".into()),
            ("a1.ts".into(), "import { x } from './hub';\n".into()),
        ];
        for i in 0..17 {
            files.push((format!("b{i}.ts"), "export const y = 1;\n".into()));
        }
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let (_dir, diags) = run_on_project(&borrowed, "hub.ts");
        assert!(diags.is_empty(), "ratio 2/20 = 10% < 30% should not fire");
    }

    #[test]
    fn allows_module_below_min_importers_even_if_ratio_high() {
        // 1 hub + 3 importers in a 4-file project = 75% ratio but only 3
        // absolute importers — below the default `min_importers` = 10.
        let files: Vec<(&str, &str)> = vec![
            ("hub.ts", "export const x = 1;"),
            ("a.ts", "import { x } from './hub';"),
            ("b.ts", "import { x } from './hub';"),
            ("c.ts", "import { x } from './hub';"),
        ];
        let (_dir, diags) = run_on_project(&files, "hub.ts");
        assert!(
            diags.is_empty(),
            "absolute importer count < min_importers gates the rule"
        );
    }

    #[test]
    fn ignores_file_with_no_importers() {
        // Standalone file, no importers. Must stay silent.
        let files: Vec<(&str, &str)> = vec![
            ("hub.ts", "export const x = 1;"),
            ("other.ts", "export const y = 2;"),
        ];
        let (_dir, diags) = run_on_project(&files, "hub.ts");
        assert!(diags.is_empty());
    }
}
