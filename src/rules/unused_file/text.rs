//! unused-file backend — flag files unreachable from any entry point.
//!
//! Runs once per project (anchored on the lexicographically smallest indexed
//! path). Emits one diagnostic per unreachable file in a single pass.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::ImportIndex;
use crate::rules::backend::{CheckCtx, TextCheck};
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

        // Once-per-project guard: only fire on the deterministic anchor.
        let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());
        let Some(anchor) = index.indexed_paths().min() else {
            return Vec::new();
        };
        if canon.as_path() != anchor {
            return Vec::new();
        }

        let project_root = ctx.project.project_root.as_deref();
        let entry_points = detect_entry_points(index, project_root);
        if entry_points.is_empty() {
            return Vec::new();
        }

        let reachable = index.reachable_from(&entry_points);

        let mut diagnostics = Vec::new();
        for path in index.indexed_paths() {
            if reachable.contains(path) {
                continue;
            }
            if is_entry_point(path, project_root) {
                continue;
            }
            if is_declaration_file(path) || is_config_file(path) || is_test_file(path) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: path.to_path_buf(),
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

fn detect_entry_points<'a>(
    index: &'a ImportIndex,
    project_root: Option<&Path>,
) -> Vec<&'a Path> {
    index
        .indexed_paths()
        .filter(|p| is_entry_point(p, project_root))
        .collect()
}

fn is_entry_point(path: &Path, project_root: Option<&Path>) -> bool {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    if is_config_file(path) {
        return true;
    }

    // Framework directories: boundary-checked with `/dir/` to avoid
    // substring false positives like `package_pages_helper.ts`.
    let path_str = path.to_str().unwrap_or("");
    let framework_dirs = [
        "/pages/", "/app/", "/routes/", "/middleware/", "/api/", "/server/",
    ];
    for dir in framework_dirs {
        if path_str.contains(dir) {
            return true;
        }
    }

    // Root-level `main` / `index` files.
    if let Some(root) = project_root
        && let Some(parent) = path.parent()
    {
        let canon_parent =
            std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        let canon_root =
            std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        if canon_parent == canon_root && (stem == "main" || stem == "index") {
            return true;
        }
    }

    false
}

fn is_declaration_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".d.ts"))
}

fn is_config_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    // `foo.config.ts`, `jest.config.js`
    if stem.ends_with(".config") {
        return true;
    }
    // `.eslintrc.js`, `.babelrc.ts` — dotfile ending in `rc`
    if name.starts_with('.') && stem.ends_with("rc") {
        return true;
    }
    false
}

fn is_test_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let path_str = path.to_str().unwrap_or("");
    name.contains(".test.") || name.contains(".spec.")
        || path_str.contains("/__tests__/") || path_str.contains("/tests/")
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
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx,
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
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const x = 1;\n"),
        ];
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
}
