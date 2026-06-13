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

        let canon = index.canonical(ctx.path);
        let Some(anchor) = ctx.project.anchor_path() else {
            return Vec::new();
        };
        if canon != anchor {
            return Vec::new();
        }

        if ctx.project.nearest_package_json(ctx.path).is_some_and(|pkg| pkg.is_library) {
            return Vec::new();
        }

        // `project_root` and `workspace_roots` are constant for the whole run,
        // but `is_entry_point` is called once per indexed path (twice, counting
        // the reachability seed pass). Canonicalizing them here collapses an
        // O(files × workspace_roots) burst of `canonicalize` syscalls — the
        // dominant cost on large monorepos — into O(workspace_roots).
        let canon_root: Option<std::path::PathBuf> = ctx
            .project
            .project_root
            .as_deref()
            .map(|r| std::fs::canonicalize(r).unwrap_or_else(|_| r.to_path_buf()));
        // A `HashSet` so the workspace-root membership test in `is_entry_point`
        // is O(1) instead of a linear scan per indexed path — a monorepo can
        // declare hundreds of workspace roots, making that scan O(files × roots).
        let canon_workspace_roots: std::collections::HashSet<std::path::PathBuf> = ctx
            .project
            .workspace_roots
            .iter()
            .map(|wr| std::fs::canonicalize(wr).unwrap_or_else(|_| wr.clone()))
            .collect();

        let entry_points =
            detect_entry_points(index, ctx.project, canon_root.as_deref(), &canon_workspace_roots);
        if entry_points.is_empty() {
            return Vec::new();
        }

        let reachable = index.reachable_from(&entry_points);

        let mut diagnostics = Vec::new();
        for path in index.indexed_paths() {
            if reachable.contains(path) {
                continue;
            }
            if is_entry_point(path, ctx.project, canon_root.as_deref(), &canon_workspace_roots) {
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

fn detect_entry_points<'a>(
    index: &'a ImportIndex,
    project: &ProjectCtx,
    canon_root: Option<&Path>,
    canon_workspace_roots: &std::collections::HashSet<std::path::PathBuf>,
) -> Vec<&'a Path> {
    index
        .indexed_paths()
        .filter(|p| {
            is_entry_point(p, project, canon_root, canon_workspace_roots)
                || is_test_file(p)
                || project.entrypoints_contains(p)
        })
        .collect()
}

fn is_entry_point(
    path: &Path,
    project: &ProjectCtx,
    canon_root: Option<&Path>,
    canon_workspace_roots: &std::collections::HashSet<std::path::PathBuf>,
) -> bool {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    if is_config_file(path) {
        return true;
    }

    if is_framework_entry_point(path, project) {
        return true;
    }

    // `main.{ts,js,mts,tsx,...}` is universally a bootstrapper/entry point: it is
    // launched directly (by a runtime or a test runner), never imported. Multi-app
    // monorepos (NestJS integration tests, Nx, Turborepo) place a `src/main.ts` in
    // every app subdirectory at arbitrary depth, so depth is not a reliable signal
    // — the stem is. (`index` stays depth-restricted below: a barrel `index.ts`
    // exists at every level and must not become a blanket entry seed.)
    if stem == "main" {
        return true;
    }

    let Some(canon_root) = canon_root else {
        return false;
    };

    // CLIs and smoke/build tools are run directly, never imported. They live in
    // a top-level `scripts/`, `bin/`, `examples/`, `example-apps/`, `tools/`,
    // or `benchmarks/` directory.
    if in_top_level_dir(
        path,
        canon_root,
        &["scripts", "bin", "examples", "example-apps", "tools", "benchmarks"],
    ) {
        return true;
    }

    let Some(parent) = path.parent() else {
        return false;
    };
    let canon_parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let at_root = canon_parent == canon_root;
    // A bundler/CLI entry conventionally sits at the project root *or* directly
    // under the source root — `index.ts`, `src/index.ts`.
    let under_src = canon_parent.parent() == Some(canon_root)
        && canon_parent.file_name().and_then(|n| n.to_str()) == Some("src");

    if stem == "index" && (at_root || under_src) {
        return true;
    }
    // Framework root file stems (e.g. "next.config" matches next.config.ts).
    if at_root && project.framework_root_files().any(|s| s == stem) {
        return true;
    }

    // Workspace package entry points: treat index.ts at the root of any
    // workspace package (or its src/ subdir) as a BFS seed, so files reachable
    // only within that package are not flagged.
    if stem == "index" {
        let at_wr = canon_workspace_roots.contains(&canon_parent);
        let under_wr_src = canon_parent.file_name().and_then(|n| n.to_str()) == Some("src")
            && canon_parent
                .parent()
                .is_some_and(|p| canon_workspace_roots.contains(p));
        if at_wr || under_wr_src {
            return true;
        }
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

    // Regression for #776: workspace package entry points and their internal
    // imports must not be flagged on monorepos.
    #[test]
    fn workspace_package_internal_files_are_not_flagged() {
        // packages/b is a workspace package not imported by the root index.ts.
        // packages/b/index.ts imports ./lib internally — both must be silent.
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"root","workspaces":["packages/*"]}"#,
            ),
            ("index.ts", "export const root = 1;\n"),
            // packages/b needs its own package.json for resolve_workspace_roots
            // to recognise it as a workspace package root.
            ("packages/b/package.json", r#"{"name":"b","version":"0.0.1"}"#),
            (
                "packages/b/index.ts",
                "import { lib } from './lib';\nexport { lib };\n",
            ),
            ("packages/b/lib.ts", "export const lib = 42;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "workspace package files must not be flagged: {diags:?}"
        );
    }

    // Regression for #776: files under top-level `examples/` are entry points.
    #[test]
    fn examples_dir_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            ("examples/bun/src/client.ts", "console.log('example');\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "examples/ files are entry points and must not be flagged: {diags:?}"
        );
    }

    // Regression for #776: files under top-level `tools/` are entry points.
    #[test]
    fn tools_dir_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            ("tools/gulp/gulpfile.ts", "console.log('task runner');\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "tools/ files are entry points and must not be flagged: {diags:?}"
        );
    }

    // Regression for #776: files under top-level `example-apps/` are entry points.
    #[test]
    fn example_apps_dir_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            (
                "example-apps/credential-sync/constants.ts",
                "export const API_URL = 'https://example.com';\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "example-apps/ files are entry points and must not be flagged: {diags:?}"
        );
    }

    // Regression for #1115: TypeScript ESM (`NodeNext`/`Bundler`) requires
    // writing the emitted `.js` extension in specifiers even when the on-disk
    // source is `.ts`/`.tsx`. The import-graph walker must resolve `./toolbar.js`
    // to `toolbar.tsx`, so the importer and its `.js`-imported `.tsx` sources
    // stay reachable instead of being orphaned.
    #[test]
    fn resolves_js_extension_imports_to_tsx_sources() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "import { theme } from './theme/theme.js';\nexport { theme };\n"),
            (
                "theme/theme.ts",
                "import { toolbar } from './toolbar.js';\n\
                 import { versionPicker, versionPickerScript } from './versionPicker.js';\n\
                 export const theme = { toolbar, versionPicker, versionPickerScript };\n",
            ),
            ("theme/toolbar.tsx", "export const toolbar = () => null;\n"),
            (
                "theme/versionPicker.tsx",
                "export const versionPicker = () => null;\nexport const versionPickerScript = '';\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "`.js`-extensioned ESM imports must resolve to their `.tsx` sources, \
             keeping the importer and imported files reachable: {diags:?}"
        );
    }

    // Regression for #1062: a nested integration-test app entry (`src/main.ts`
    // at arbitrary depth) is a bootstrapper launched directly, not imported — it
    // and its reachable module tree must not be flagged.
    #[test]
    fn treats_nested_app_main_as_entry_point() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            (
                "integration/microservices/src/main.ts",
                "import { ApplicationModule } from './app.module';\nvoid ApplicationModule;\n",
            ),
            (
                "integration/microservices/src/app.module.ts",
                "export const ApplicationModule = class {};\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "nested app main.ts and its module tree must not be flagged: {diags:?}"
        );
    }

    // Regression for #1402: Next.js page files under a `pages/` directory are
    // consumed by the framework's file-system router — nothing imports them
    // statically. They must not be flagged, including special files like
    // `_app.js`. The Next.js app is nested under a library's `app/` directory
    // (like formik), so `next` lives in the nested package.json, not the root.
    #[test]
    fn nextjs_pages_dir_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"formik","main":"dist/index.js"}"#),
            ("src/index.ts", "export const formik = 1;\n"),
            ("app/package.json", r#"{"name":"formik-app","dependencies":{"next":"14.0.0"}}"#),
            ("app/pages/index.tsx", "export default function Home() { return null; }\n"),
            ("app/pages/_app.js", "export default function App({ Component }) { return null; }\n"),
            ("app/pages/sign-in.js", "export default function SignIn() { return null; }\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "Next.js pages/ files are framework-routed entry points: {diags:?}"
        );
    }

    // Regression for #1402: the framework-routing exemption must not blanket
    // the whole project — a genuinely orphaned non-route file (imported by
    // nothing, outside any framework routing directory) is still a true
    // positive even when the project contains a Next.js app.
    #[test]
    fn orphan_file_outside_nextjs_routes_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"site","dependencies":{"next":"14.0.0"}}"#,
            ),
            ("src/index.ts", "export const site = 1;\n"),
            ("pages/index.tsx", "export default function Home() { return null; }\n"),
            ("src/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "diagnostic should target the orphaned non-route file: {diags:?}"
        );
    }
}
