//! duplicate-export detection — within each npm package, aggregate every
//! `ReExport` and flag symbol names that show up in two or more distinct barrel
//! files of that package.
//!
//! Re-exports are grouped by the nearest `package.json` directory so that two
//! independent packages re-exporting the same symbol name (e.g. a shared `LogLevel`
//! enum across separate workspace packages) are never compared — only barrels
//! inside the same package create an ambiguous import path.
//!
//! Skips:
//!   - `"default"` re-exports — barrels routinely re-export a default under
//!     different names; treating that as ambiguous would be noise.
//!   - `"*"` star re-exports — they don't carry a specific name to compare.
//!   - Symbols that appear in only one barrel within a package — no duplication.
//!
//! Runs once per project, anchored on the lexicographically smallest indexed
//! path so that a single pass emits all diagnostics deterministically. Barrel
//! paths in the message are shown relative to the project root.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::ExportKind;
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const RULE_ID: &str = "duplicate-export";

/// Per-package re-export occurrences keyed by `(package dir, symbol name)`,
/// each holding the barrel files and lines where the name is re-exported.
type ReExportMap = HashMap<(Option<PathBuf>, String), Vec<(PathBuf, usize)>>;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let index = ctx.project.import_index();

        let canon = index.canonical(ctx.path);
        let Some(anchor) = ctx.project.anchor_path() else {
            return Vec::new();
        };
        if canon != anchor {
            return Vec::new();
        }

        // (package dir, symbol name) -> list of (barrel file, line) where it is
        // re-exported. Keying on the nearest `package.json` directory keeps
        // independent packages from being compared against each other; barrels
        // outside any package share a `None` key and compare among themselves.
        let mut reexports: ReExportMap = HashMap::new();
        for (path, exports) in index.iter_exports() {
            for export in exports {
                if !matches!(export.kind, ExportKind::ReExport) {
                    continue;
                }
                if export.name == "default" || export.name == "*" {
                    continue;
                }
                let package_dir = ctx.project.nearest_package_json_dir(path);
                reexports
                    .entry((package_dir, export.name.clone()))
                    .or_default()
                    .push((path.to_path_buf(), export.line));
            }
        }

        // Indexed barrel paths are canonical; canonicalize the project root once
        // so message paths strip cleanly to project-relative form.
        let root = ctx
            .project
            .project_root
            .as_deref()
            .and_then(|r| std::fs::canonicalize(r).ok());

        let mut diagnostics = Vec::new();
        let mut keys: Vec<&(Option<PathBuf>, String)> = reexports.keys().collect();
        keys.sort();
        for key in keys {
            let (_, name) = key;
            let occurrences = &reexports[key];
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
                .map(|p| format!("`{}`", display_path(p, root.as_deref())))
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

/// Render `path` relative to `root` for the diagnostic message, falling back to
/// the full path when it lies outside the root. Keeps the comply install's
/// absolute path out of user-facing output.
fn display_path(path: &Path, root: Option<&Path>) -> String {
    root.and_then(|r| path.strip_prefix(r).ok())
        .unwrap_or(path)
        .display()
        .to_string()
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
            // Non-source manifests (e.g. `package.json`) are written to disk so
            // package-boundary detection sees them, but not indexed as sources.
            if let Some(lang) = Language::from_path(&p) {
                source_files.push(SourceFile { path: p, language: lang });
            }
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
            file: &file_ctx, lang: crate::files::Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        (dir, diags)
    }

    /// Pick the source file the project's anchor rule will land on so the
    /// once-per-project guard fires inside `run_on_project`. Non-source
    /// manifests (e.g. `package.json`) aren't indexed, so they're excluded.
    fn anchor_rel<'a>(files: &'a [(&'a str, &'a str)]) -> &'a str {
        files
            .iter()
            .map(|(rel, _)| *rel)
            .filter(|rel| Language::from_path(Path::new(rel)).is_some())
            .min()
            .expect("at least one source file")
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

    /// #1082: in a monorepo, two independent packages re-exporting a symbol of
    /// the same name (e.g. `LogLevel`) are separate namespaces and must not be
    /// flagged as duplicates.
    #[test]
    fn allows_same_symbol_across_distinct_packages() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"monorepo","private":true}"#),
            ("sdk/pkg-a/package.json", r#"{"name":"@scope/pkg-a"}"#),
            ("sdk/pkg-a/src/log.ts", "export enum LogLevel { Info }"),
            ("sdk/pkg-a/index.ts", "export { LogLevel } from './src/log';"),
            ("sdk/pkg-b/package.json", r#"{"name":"@scope/pkg-b"}"#),
            ("sdk/pkg-b/src/log.ts", "export enum LogLevel { Error }"),
            ("sdk/pkg-b/index.ts", "export { LogLevel } from './src/log';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "same symbol re-exported by distinct packages must not be flagged, got: {:?}",
            diags
        );
    }

    /// Two barrels *inside the same package* re-exporting one symbol still
    /// create an ambiguous import path within that package — keep flagging.
    #[test]
    fn flags_two_barrels_within_one_package() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"single-pkg"}"#),
            ("src/impl.ts", "export function compute() {}"),
            ("barrel1.ts", "export { compute } from './src/impl';"),
            ("barrel2.ts", "export { compute } from './src/impl';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(diags.len(), 1, "same-package duplicate must flag, got: {:?}", diags);
        assert!(diags[0].message.contains("compute"));
    }

    /// #1082: barrel paths in the message are relative to the project root —
    /// the comply install's absolute path never leaks.
    #[test]
    fn message_uses_paths_relative_to_project_root() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"single-pkg"}"#),
            ("src/impl.ts", "export function compute() {}"),
            ("barrel1.ts", "export { compute } from './src/impl';"),
            ("barrel2.ts", "export { compute } from './src/impl';"),
        ];
        let target = anchor_rel(&files);
        let (dir, diags) = run_on_project(&files, target);
        assert_eq!(diags.len(), 1);
        let msg = &diags[0].message;
        assert!(
            !msg.contains(&dir.path().display().to_string()),
            "message must not contain the absolute project path, got: {}",
            msg
        );
        assert!(
            msg.contains("`barrel1.ts`") && msg.contains("`barrel2.ts`"),
            "message should list package-relative barrel paths, got: {}",
            msg
        );
    }
}
