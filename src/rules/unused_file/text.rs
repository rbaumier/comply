//! unused-file backend — flag files unreachable from any entry point.
//!
//! Runs once per project (anchored on the lexicographically smallest indexed
//! path). Emits one diagnostic per unreachable file in a single pass.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::{ImportIndex, ProjectCtx};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::path_utils::{is_config_file, is_framework_entry_point};
use std::path::Path;

const RULE_ID: &str = "unused-file";

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let index = ctx.project.import_index();
        if index.is_empty() {
            return Vec::new();
        }

        let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());
        let Some(anchor) = ctx.project.anchor_path() else {
            return Vec::new();
        };
        if canon != anchor {
            return Vec::new();
        }

        if ctx.project.nearest_package_json(ctx.path).is_some_and(|pkg| pkg.is_library) {
            return Vec::new();
        }

        let entry_points = detect_entry_points(index, ctx.project);
        if entry_points.is_empty() {
            return Vec::new();
        }

        let reachable = index.reachable_from(&entry_points);

        let mut diagnostics = Vec::new();
        for path in index.indexed_paths() {
            if reachable.contains(path) {
                continue;
            }
            if is_entry_point(path, ctx.project) {
                continue;
            }
            if is_declaration_file(path)
                || is_config_file(path)
                || is_test_file(path)
                || is_in_ui_library(path)
                || is_in_fixture_dir(path)
            {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: path.to_path_buf().into(),
                line: 1,
                column: 1,
                rule_id: RULE_ID.into(),
                message: "File is not reachable from any entry point via the import graph."
                    .to_string(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

fn detect_entry_points<'a>(index: &'a ImportIndex, project: &ProjectCtx) -> Vec<&'a Path> {
    index
        .indexed_paths()
        .filter(|p| is_entry_point(p, project) || is_test_file(p) || project.entrypoints_contains(p))
        .collect()
}

fn is_entry_point(path: &Path, project: &ProjectCtx) -> bool {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    if is_config_file(path) {
        return true;
    }

    if is_framework_entry_point(path, project) {
        return true;
    }

    let Some(root) = project.project_root.as_deref() else {
        return false;
    };
    let canon_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

    // CLIs and smoke/build tools are run directly, never imported. They live in
    // a top-level `scripts/` or `bin/` directory.
    if in_top_level_dir(path, &canon_root, &["scripts", "bin"]) {
        return true;
    }

    let Some(parent) = path.parent() else {
        return false;
    };
    let canon_parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let at_root = canon_parent == canon_root;
    // A bundler/CLI entry conventionally sits at the project root *or* directly
    // under the source root — `main.ts`, `index.ts`, `src/main.ts`.
    let under_src = canon_parent.parent() == Some(canon_root.as_path())
        && canon_parent.file_name().and_then(|n| n.to_str()) == Some("src");

    if (stem == "main" || stem == "index") && (at_root || under_src) {
        return true;
    }
    // Framework root file stems (e.g. "next.config" matches next.config.ts).
    if at_root && project.framework_root_files().any(|s| s == stem) {
        return true;
    }

    false
}

/// True when `path` lives anywhere inside one of `dirs` taken as a top-level
/// directory of the project (e.g. `<root>/scripts/cli.ts`).
fn in_top_level_dir(path: &Path, canon_root: &Path, dirs: &[&str]) -> bool {
    let canon_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let Ok(rel) = canon_path.strip_prefix(canon_root) else {
        return false;
    };
    rel.components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
        .is_some_and(|first| dirs.contains(&first))
}

fn is_declaration_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".d.ts"))
}

fn is_test_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let path_str = path.to_str().unwrap_or("");
    name.contains(".test.")
        || name.contains(".spec.")
        || path_str.contains("/__tests__/")
        || path_str.contains("/tests/")
}

fn is_in_ui_library(path: &Path) -> bool {
    let path_str = path.to_str().unwrap_or("");
    path_str.contains("/components/ui/") || path_str.contains("/lib/ui/")
}

fn is_in_fixture_dir(path: &Path) -> bool {
    let path_str = path.to_str().unwrap_or("");
    path_str.contains("__testfixtures__")
        || path_str.contains("__fixtures__")
        || path_str.contains("/fixtures/")
        || path_str.contains("/test-fixtures/")
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

    fn run_on_project(files: &[(&str, &str)]) -> (TempDir, Vec<Diagnostic>) {
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

        let target_path: PathBuf = project
            .import_index()
            .indexed_paths()
            .min()
            .expect("at least one indexed file")
            .to_path_buf();
        let source = fs::read_to_string(&target_path).unwrap();
        let language = Language::from_path(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, language, &project);
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

    #[test]
    fn flags_unreachable_file() {
        // index.ts → a.ts → b.ts; orphan.ts is unreachable.
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "import { a } from './a';\n"),
            ("a.ts", "import { b } from './b';\nexport const a = b;\n"),
            ("b.ts", "export const b = 1;\n"),
            ("orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic");
        assert_eq!(diags[0].rule_id, "unused-file");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "diagnostic should target orphan.ts"
        );
    }

    #[test]
    fn allows_reachable_file() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "import { a } from './a';\n"),
            ("a.ts", "import { b } from './b';\nexport const a = b;\n"),
            ("b.ts", "export const b = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(diags.is_empty(), "all files are reachable from index.ts");
    }

    #[test]
    fn allows_entry_point_itself() {
        let files: Vec<(&str, &str)> = vec![("index.ts", "export const x = 1;\n")];
        let (_dir, diags) = run_on_project(&files);
        assert!(diags.is_empty(), "entry points are exempt by definition");
    }

    #[test]
    fn skips_test_files() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const x = 1;\n"),
            ("foo.test.ts", "export const y = 2;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(diags.is_empty(), "test files are exempt");
    }

    // Regression for #277: `src/main.ts` is an entry point, so its transitive
    // imports are reachable and must not be flagged.
    #[test]
    fn treats_src_main_as_entry_point() {
        let files: Vec<(&str, &str)> = vec![
            ("src/main.ts", "import { run } from './session/tmux';\nrun();\n"),
            ("src/session/tmux.ts", "export const run = () => {};\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(diags.is_empty(), "src/main.ts and its imports are reachable: {diags:?}");
    }

    // Regression for #336: a test helper imported only from *.test.tsx files must
    // not be flagged. Test files are now BFS roots, so their imports are reachable.
    #[test]
    fn test_helper_imported_from_test_file_is_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const x = 1;\n"),
            (
                "src/app/test-helpers/mount-columns-table.tsx",
                "export const mountColumnsTable = () => {};\n",
            ),
            (
                "src/app/features/laboratories/laboratories-columns.test.tsx",
                "import { mountColumnsTable } from '../../test-helpers/mount-columns-table';\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "test helper imported from a test file must not be flagged: {diags:?}"
        );
    }

    // Regression for #277: CLIs/smoke tools under `scripts/` are run directly.
    #[test]
    fn treats_scripts_dir_files_as_entry_points() {
        let files: Vec<(&str, &str)> = vec![
            ("src/main.ts", "export const x = 1;\n"),
            ("scripts/cli.ts", "console.log('run me');\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(diags.is_empty(), "scripts/ files are entry points: {diags:?}");
    }

    // Regression for #496: unused-file diagnostics must carry absolute paths
    // (from the import index, which canonicalizes all paths). is_rule_enabled
    // must be able to match those absolute paths against relative override globs.
    #[test]
    fn diagnostic_paths_are_absolute() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "import { a } from './a';\n"),
            ("a.ts", "export const a = 1;\n"),
            ("src/app/components/data-table/body.tsx", "export const body = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.is_absolute(),
            "unused-file diagnostic path must be absolute so that is_rule_enabled \
             can relativize it against CWD for override glob matching: {:?}",
            diags[0].path
        );
    }
}
