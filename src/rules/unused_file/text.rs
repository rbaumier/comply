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
        let jest_base_config_dirs = jest_base_config_dirs(index);

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
                || is_jest_config_variant(path, &jest_base_config_dirs)
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
                || project.is_package_entry_file(p)
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

    if is_standalone_sample_script(path, stem) {
        return true;
    }

    let Some(canon_root) = canon_root else {
        return false;
    };

    // CLIs and smoke/build tools are run directly, never imported. They live in
    // a top-level `scripts/`, `bin/`, `examples/`, `example-apps/`, `tools/`,
    // `benchmark/`, or `benchmarks/` directory.
    if in_top_level_dir(
        path,
        canon_root,
        &["scripts", "bin", "examples", "example-apps", "tools", "benchmark", "benchmarks"],
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

/// True for a standalone sample script: a `*Sample.{ts,js,...}` file living
/// under a `samples/` or `samples-dev/` directory. The Azure ARM SDK (and
/// other SDK generators) emit one such file per API operation; each is a
/// self-contained executable that defines `main()` and calls
/// `main().catch(...)` at the top level, run directly via `ts-node`. Nothing
/// imports them, so they are their own entry points, not dead code. The
/// `Sample` stem suffix is paired with the directory check so an ordinary
/// source module that merely happens to end in `Sample` stays a true positive.
fn is_standalone_sample_script(path: &Path, stem: &str) -> bool {
    if !stem.ends_with("Sample") {
        return false;
    }
    path.components().any(|c| {
        matches!(c.as_os_str().to_str(), Some("samples") | Some("samples-dev"))
    })
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
        || name.contains(".setup.")
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

/// Directories that hold a base Jest config (`jest.config.js`, `jest.config.ts`,
/// `jest.config.mjs`, …). A `jest.<name>.<ext>` variant sitting beside one of
/// these is a Jest alternate config (see [`is_jest_config_variant`]).
fn jest_base_config_dirs(index: &ImportIndex) -> rustc_hash::FxHashSet<&Path> {
    index
        .indexed_paths()
        .filter(|p| p.file_stem().and_then(|s| s.to_str()) == Some("jest.config"))
        .filter_map(|p| p.parent())
        .collect()
}

/// True for a Jest alternate config file: a `jest.<name>.<ext>` file (e.g.
/// `jest.dist.js`, `jest.prod.js`) sitting in a directory that also holds a
/// base `jest.config.*`. Such files are loaded directly via the Jest CLI
/// `-c <path>` flag, never `import`ed, so the import graph cannot reach them.
/// The sibling-base gate keeps a stray unrelated `jest.foo.js` (no base config
/// present) a true positive. Names that are themselves `jest.config.*` are
/// already exempt via [`is_config_file`].
fn is_jest_config_variant(path: &Path, base_config_dirs: &rustc_hash::FxHashSet<&Path>) -> bool {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    // `jest.<name>` with a non-empty `<name>` after the `jest.` prefix.
    if !stem.starts_with("jest.") || stem.len() <= "jest.".len() {
        return false;
    }
    path.parent()
        .is_some_and(|parent| base_config_dirs.contains(parent))
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
    fn allows_files_under_template_literal_dynamic_import_dir_issue_1789() {
        // Regression for #1789 (chakra-ui): every file under
        // `apps/compositions/src/` is loaded ONLY via the template-literal
        // dynamic import in `example.tsx`, never via a static `import`. They must
        // not be flagged as unreachable. A file outside the prefix still is.
        let files: Vec<(&str, &str)> = vec![
            (
                "index.ts",
                "import { ExamplePreview } from './components/example';\nExamplePreview();\n",
            ),
            (
                "components/example.tsx",
                "import dynamic from 'next/dynamic';\n\
                 export const ExamplePreview = (props) => {\n\
                   const { name, scope = 'examples' } = props;\n\
                   const Component = dynamic(() =>\n\
                     import(`../compositions/src/${scope}/${name}`).then(\n\
                       (mod) => mod[name] || mod.default,\n\
                     ),\n\
                   );\n\
                   return Component;\n\
                 };\n",
            ),
            (
                "compositions/src/ui/steps.tsx",
                "export const StepsRoot = 1;\nexport const StepsList = 2;\n",
            ),
            (
                "compositions/src/examples/badge-basic.tsx",
                "export const BadgeBasic = 3;\n",
            ),
            ("orphan.tsx", "export const orphan = 4;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        let flagged: Vec<&str> = diags.iter().filter_map(|d| d.path.to_str()).collect();
        assert!(
            !flagged.iter().any(|p| p.contains("compositions/src")),
            "files under the dynamic-import dir must not be flagged: {flagged:?}"
        );
        assert!(
            flagged.iter().any(|p| p.contains("orphan")),
            "a genuinely unreachable file outside the prefix is still flagged: {flagged:?}"
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

    // Regression for #1685: multi-level workspace globs (e.g. redwood's
    // `packages/auth-providers/*/*`) must seed the entry points of packages
    // nested several directories deep. A genuinely orphaned file outside any
    // workspace package is still flagged.
    #[test]
    fn multi_level_workspace_package_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"root","workspaces":["packages/*","packages/auth-providers/*/*"]}"#,
            ),
            ("index.ts", "export const root = 1;\n"),
            // Two-level nested package: packages/auth-providers/azure/web.
            (
                "packages/auth-providers/azure/web/package.json",
                r#"{"name":"@rw/azure-web","version":"0.0.1"}"#,
            ),
            (
                "packages/auth-providers/azure/web/src/index.ts",
                "import { azure } from './azure';\nexport { azure };\n",
            ),
            (
                "packages/auth-providers/azure/web/src/azure.ts",
                "export const azure = 1;\n",
            ),
            // Genuinely orphaned source: not under any workspace package, not
            // imported by anything — must still be flagged.
            ("src/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(
            diags.len(),
            1,
            "only the orphan must be flagged, nested package files must be seeded: {diags:?}"
        );
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "the flagged file must be the orphan, not a nested package file: {diags:?}"
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

    // Regression for #2066: files under a top-level `benchmark/` directory
    // (singular) are benchmark entry points run directly, like `benchmarks/`.
    #[test]
    fn benchmark_dir_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            (
                "benchmark/babel-traverse/enter-visitor-context-change/bench.mjs",
                "import Benchmark from 'benchmark';\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "top-level benchmark/ files are entry points and must not be flagged: {diags:?}"
        );
    }

    // The `benchmark/` exemption is top-level only: an unimported `benchmark`
    // directory nested under `src/` is regular source and must still be flagged.
    #[test]
    fn nested_benchmark_dir_file_is_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            ("src/benchmark/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "nested src/benchmark/ orphan must still be flagged: {diags:?}"
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

    // Regression for #1415: SvelteKit route files use a `+` prefix
    // (`+page.svelte`, `+page.server.ts`, `+server.ts`, …) under a `routes/`
    // directory. The framework's file-system router consumes them by path —
    // nothing imports them — so they must not be flagged as unused.
    #[test]
    fn sveltekit_route_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"app","dependencies":{"@sveltejs/kit":"2.0.0"}}"#,
            ),
            ("src/index.ts", "export const app = 1;\n"),
            ("src/routes/+page.svelte", "<h1>Home</h1>\n"),
            (
                "src/routes/+page.server.ts",
                "export const load = () => ({});\n",
            ),
            (
                "src/routes/read/+server.js",
                "export function GET() { return new Response('ok'); }\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "SvelteKit `+`-prefixed route files are framework-routed entry points: {diags:?}"
        );
    }

    // Regression for #1415: the SvelteKit exemption is precise — an ordinary
    // never-imported `.ts` file outside the `routes/` convention is still a
    // true positive even when the project is a SvelteKit app.
    #[test]
    fn orphan_file_in_sveltekit_app_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"app","dependencies":{"@sveltejs/kit":"2.0.0"}}"#,
            ),
            ("src/index.ts", "export const app = 1;\n"),
            ("src/routes/+page.svelte", "<h1>Home</h1>\n"),
            ("src/lib/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "diagnostic should target the orphaned non-route file: {diags:?}"
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

    // Regression for #1989: Cypress spec files (**/*.cy.{ts,js,tsx,jsx}) are
    // discovered via specPattern and executed by the runner, and the
    // cypress/support/e2e.* and commands.* files are auto-imported before each
    // suite. None are reachable through the import graph, so none must be flagged.
    #[test]
    fn cypress_spec_and_support_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"app","devDependencies":{"cypress":"13.0.0"}}"#,
            ),
            ("src/index.ts", "export const app = 1;\n"),
            ("cypress/e2e/Select.cy.ts", "describe('Select', () => {});\n"),
            ("cypress/support/e2e.js", "import './commands';\n"),
            ("cypress/support/commands.js", "Cypress.Commands.add('login', () => {});\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "Cypress spec and support files are test-runner entry points: {diags:?}"
        );
    }

    // Regression for #1989: the Cypress exemption is precise — a genuinely
    // orphaned normal source file is still a true positive even when the
    // project uses Cypress.
    #[test]
    fn orphan_file_in_cypress_project_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"app","devDependencies":{"cypress":"13.0.0"}}"#,
            ),
            ("src/index.ts", "export const app = 1;\n"),
            ("cypress/e2e/Select.cy.ts", "describe('Select', () => {});\n"),
            ("src/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "diagnostic should target the orphaned non-Cypress file: {diags:?}"
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

    // Regression for #1850: in a Yarn/npm/pnpm workspace monorepo, one package
    // imports another by its package NAME (`import { x } from "motion-utils"`).
    // The root package.json has no main/exports (is_library=false, rule runs),
    // but the depended-upon package's source files ARE reachable through the
    // cross-package name import. Resolution must map the workspace package name
    // to its on-disk source entry so those files are not flagged as unused.
    #[test]
    fn cross_package_name_imports_keep_sibling_source_reachable() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"workspaces":["packages/*","dev/*"],"packageManager":"yarn@3.6.4"}"#,
            ),
            // A non-library dev app — its package.json has no main/exports, so it
            // is the anchor (smallest indexed path) and the rule is NOT globally
            // skipped. It pulls in motion-dom by name.
            (
                "dev/app/package.json",
                r#"{"name":"dev-app"}"#,
            ),
            (
                "dev/app/index.ts",
                "import { useIt } from 'motion-dom';\nuseIt();\n",
            ),
            (
                "packages/motion-utils/package.json",
                r#"{"name":"motion-utils","main":"./dist/cjs/index.js","module":"./dist/es/index.mjs"}"#,
            ),
            (
                "packages/motion-utils/src/index.ts",
                "export { isEasingArray } from './easing/utils/is-easing-array';\n",
            ),
            (
                "packages/motion-utils/src/easing/utils/is-easing-array.ts",
                "export const isEasingArray = (ease: unknown): boolean => Array.isArray(ease);\n",
            ),
            (
                "packages/motion-dom/package.json",
                r#"{"name":"motion-dom","main":"./dist/cjs/index.js"}"#,
            ),
            (
                "packages/motion-dom/src/index.ts",
                "import { isEasingArray } from 'motion-utils';\nexport const useIt = () => isEasingArray([]);\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "source files of a workspace package imported by its name must stay reachable: {diags:?}"
        );
    }

    // Regression for #1850: the cross-package resolution is precise — a genuinely
    // orphaned file inside a workspace member (imported by nothing, not reachable
    // through any cross-package import) is still a true positive.
    #[test]
    fn orphan_inside_workspace_member_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"workspaces":["packages/*","dev/*"]}"#,
            ),
            // Non-library dev app: the anchor, so the rule runs.
            ("dev/app/package.json", r#"{"name":"dev-app"}"#),
            (
                "dev/app/index.ts",
                "import { used } from 'utils';\nused;\n",
            ),
            (
                "packages/utils/package.json",
                r#"{"name":"utils","main":"./dist/index.js"}"#,
            ),
            (
                "packages/utils/src/index.ts",
                "export const used = 1;\n",
            ),
            (
                "packages/utils/src/orphan.ts",
                "export const orphan = 1;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "the genuinely orphaned file inside the workspace member must be flagged: {diags:?}"
        );
    }

    // Regression for #1948: files referenced as VALUES in a package.json
    // `browser` (or `react-native`) substitution map are the browser/native
    // build bundlers swap in at build time. They are never `import`ed, so the
    // import graph cannot reach them — but they are declared entry points, not
    // dead code, and must not be flagged. A genuine orphan stays flagged.
    #[test]
    fn browser_substitute_targets_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"axios","browser":{"./lib/platform/node/index.js":"./lib/platform/browser/index.js","./lib/adapters/http.js":"./lib/helpers/null.js"}}"#,
            ),
            (
                "src/index.js",
                "import nodeIndex from '../lib/platform/node/index.js';\nexport { nodeIndex };\n",
            ),
            ("lib/platform/node/index.js", "export default {};\n"),
            // Browser substitutes — never imported, swapped in by the bundler.
            ("lib/platform/browser/index.js", "export default {};\n"),
            ("lib/helpers/null.js", "export default null;\n"),
            // A genuine orphan: not imported, not a substitute, not an entry.
            ("lib/orphan.js", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected exactly one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "only the genuine orphan must be flagged; browser substitutes are entry points: {diags:?}"
        );
    }

    // Regression for #1881: a utility module reached only through an
    // `export * from './misc/getTreeDiff'` barrel must be considered
    // reachable. The barrel is imported from an entry-rooted file, so its
    // star-re-exported sources are live via `reexport_edges`.
    #[test]
    fn file_reachable_via_star_reexport_barrel_is_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "import { getTreeDiff } from './utils';\ngetTreeDiff();\n"),
            ("src/utils/index.ts", "export * from './misc/getTreeDiff';\n"),
            (
                "src/utils/misc/getTreeDiff.ts",
                "export function getTreeDiff(): void {}\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "file reached through `export *` barrel must not be flagged unused: {diags:?}"
        );
    }

    // Regression for #1952: Jest alternate config files (`jest.dist.js`,
    // `jest.prod.js`) are loaded directly via the Jest CLI `-c <path>` flag in
    // package.json scripts, never `import`ed, so the import graph cannot reach
    // them. When a base `jest.config.*` sits beside them they are recognised as
    // Jest config variants and must not be flagged. A genuine orphan stays
    // flagged.
    #[test]
    fn jest_alternate_config_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"app","scripts":{"test:dist":"jest -c jest.dist.js"}}"#,
            ),
            ("src/index.ts", "export const app = 1;\n"),
            ("jest.config.js", "module.exports = {};\n"),
            (
                "jest.dist.js",
                "const base = require('./jest.config.js');\nmodule.exports = Object.assign({}, base);\n",
            ),
            (
                "jest.prod.js",
                "const base = require('./jest.config.js');\nmodule.exports = Object.assign({}, base);\n",
            ),
            ("orphan.js", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected exactly one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "only the genuine orphan must be flagged; Jest config variants are entry points: {diags:?}"
        );
    }

    // Regression for #1952: the Jest-variant exemption is gated on a sibling
    // base `jest.config.*` existing. Without one, a stray `jest.foo.js` is not
    // a recognised config variant and stays a true positive.
    #[test]
    fn jest_variant_without_base_config_is_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"app"}"#),
            ("src/index.ts", "export const app = 1;\n"),
            ("jest.foo.js", "module.exports = {};\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("jest.foo"),
            "a jest.* variant without a sibling base jest.config.* is still flagged: {diags:?}"
        );
    }

    // Regression for #1164: Azure ARM SDK `*Sample.ts` standalone scripts under
    // a `samples/` directory define `main()` and call `main().catch(...)` at the
    // top level — they are run directly via `ts-node`, imported by nothing, and
    // are therefore their own entry points, not dead code.
    #[test]
    fn azure_sample_scripts_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "export const app = 1;\n"),
            (
                "samples/v36/typescript/src/vpnLinkConnectionsResetConnectionSample.ts",
                "import { DefaultAzureCredential } from '@azure/identity';\n\
                 async function main(): Promise<void> {\n\
                 \x20\x20void new DefaultAzureCredential();\n\
                 }\n\
                 main().catch(console.error);\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "`*Sample.ts` scripts under samples/ are standalone entry points: {diags:?}"
        );
    }

    // Regression for #1164: the sample exemption is precise — a genuinely
    // orphaned source file that is neither a sample script nor under a
    // `samples/` directory is still a true positive.
    #[test]
    fn orphan_file_beside_samples_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "export const app = 1;\n"),
            (
                "samples/v1/typescript/src/createSample.ts",
                "async function main(): Promise<void> {}\nmain().catch(console.error);\n",
            ),
            ("src/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "only the genuine orphan must be flagged; the sample script is an entry point: {diags:?}"
        );
    }

    // Regression for #1815: test setup files (`vitest.setup.mts`,
    // `jest.setup.ts`) are referenced from the test runner config via the
    // `setupFiles` string property, not via a TypeScript `import`, so the import
    // graph cannot reach them. They are test infrastructure, not dead code, and
    // must not be flagged. `vitest.workspace.mjs` is a workspace test config
    // (already covered by `is_config_file`'s `.workspace` classifier).
    #[test]
    fn test_setup_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("vitest.config.mts", "export default { test: { setupFiles: './vitest.setup.mts' } };\n"),
            (
                "vitest.setup.mts",
                "import '@testing-library/jest-dom/vitest';\n\
                 import { cleanup } from '@testing-library/react';\n\
                 import { afterEach } from 'vitest';\n\
                 afterEach(() => { cleanup(); });\n",
            ),
            ("jest.config.js", "module.exports = { setupFilesAfterEach: ['./jest.setup.ts'] };\n"),
            ("jest.setup.ts", "import '@testing-library/jest-dom';\n"),
            ("vitest.workspace.mjs", "import { defineWorkspace } from 'vitest/config';\nexport default defineWorkspace(['./packages/*']);\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "test setup/workspace files are test-runner infrastructure, not dead code: {diags:?}"
        );
    }

    // Regression for #1815: the setup exemption is precise — a genuinely
    // orphaned ordinary source file is still a true positive even when the
    // project contains test setup files.
    #[test]
    fn orphan_file_beside_test_setup_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "export const app = 1;\n"),
            ("vitest.setup.ts", "import 'vitest';\n"),
            ("src/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "only the genuine orphan must be flagged; the setup file is test infrastructure: {diags:?}"
        );
    }

    // Regression for #1164: the `Sample` suffix is gated on the `samples/`
    // directory — an ordinary source module that merely ends in `Sample` but
    // lives outside any samples/ directory is not a standalone script and stays
    // a true positive when orphaned.
    #[test]
    fn sample_suffixed_source_outside_samples_dir_is_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "export const app = 1;\n"),
            ("src/lib/dataSample.ts", "export const dataSample = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("dataSample"),
            "a *Sample file outside any samples/ dir is not a standalone script: {diags:?}"
        );
    }

    // Regression for #1687: Docusaurus loads `src/pages/` via filesystem
    // routing, `src/theme/` via swizzling, `src/components/` via MDX injection,
    // and `versioned_docs/` components by path — none through JS imports, so the
    // import-graph BFS cannot reach them. Detection is gated on `@docusaurus/core`
    // in the nearest package.json (here the nested `docs/` site), so these dirs
    // are treated as framework entry points and must not be flagged.
    #[test]
    fn docusaurus_framework_loaded_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            (
                "docs/package.json",
                r#"{"name":"docs","private":true,"dependencies":{"@docusaurus/core":"^3.0.0"}}"#,
            ),
            ("docs/docusaurus.config.ts", "export default { title: 'Docs' };\n"),
            (
                "docs/src/pages/index.js",
                "import React from 'react';\nexport default function Home() { return null; }\n",
            ),
            (
                "docs/src/theme/Footer.tsx",
                "export default function Footer() { return null; }\n",
            ),
            (
                "docs/src/components/ReactPlayer.jsx",
                "export default function Player() { return null; }\n",
            ),
            (
                "docs/versioned_docs/version-1.0/ReactPlayer.jsx",
                "export default function Player() { return null; }\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "Docusaurus framework-loaded files must not be flagged: {diags:?}"
        );
    }

    // The Docusaurus exemption is precise: a genuinely orphaned file under a
    // non-entry-point directory of the same Docusaurus site (here `src/utils/`)
    // is still a true positive.
    #[test]
    fn docusaurus_orphan_outside_entry_dirs_is_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            (
                "docs/package.json",
                r#"{"name":"docs","private":true,"dependencies":{"@docusaurus/core":"^3.0.0"}}"#,
            ),
            ("docs/docusaurus.config.ts", "export default { title: 'Docs' };\n"),
            ("docs/src/utils/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "an orphan outside the Docusaurus entry dirs must still be flagged: {diags:?}"
        );
    }

    // Detection-gating keeps the broad `src/components/` exemption Docusaurus-only:
    // in a project WITHOUT `@docusaurus/core`, an orphaned `src/components/` file
    // is still flagged.
    #[test]
    fn components_dir_orphan_flagged_without_docusaurus() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            ("src/components/orphan.tsx", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "without Docusaurus, an orphaned src/components/ file is still flagged: {diags:?}"
        );
    }
}
