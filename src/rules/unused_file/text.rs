//! unused-file backend — flag files unreachable from any entry point.
//!
//! Runs once per project (anchored on the lexicographically smallest indexed
//! path). Emits one diagnostic per unreachable file in a single pass.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::project::{ImportIndex, ProjectCtx};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::path_utils::{
    is_angular_schematic_or_migration_entry, is_auto_mock_dir_path, is_config_file,
    is_framework_entry_point, is_sample_dir_path, is_storybook_story, is_top_level_script_dir_path,
};
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
                || is_auto_mock_dir_path(path)
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
                || is_typeorm_glob_loaded_file(p)
                || project.entrypoints_contains(p)
                || project.is_package_entry_file(p)
                || project.is_in_published_files_surface(p)
                || project.is_declared_entry_barrel(p)
        })
        .collect()
}

/// True for a TypeORM artifact registered at runtime through a DataSource glob
/// pattern (`entities`/`subscribers`/`migrations`), never `import`ed from an
/// entry point — so the import-graph BFS cannot reach it. TypeORM scans these
/// globs at load time and registers every matching file, so each is a real
/// entry point whose transitive imports are live. Seeding them keeps the file
/// (and the helpers it imports) out of the unused-file results.
///
/// The signal is the file's own content, not its directory name: a file is
/// recognised when it declares an `@Entity`/`@ViewEntity`/`@ChildEntity`/
/// `@EventSubscriber` decorator or `implements MigrationInterface`. This is
/// TypeORM-specific — a project not using TypeORM cannot carry these markers —
/// so no separate dependency gate is needed, and it holds even in the TypeORM
/// repo itself (which does not list `typeorm` as a dependency). A cheap path
/// pre-filter (the conventional `entity`/`subscriber`/`migration` directory or
/// filename) bounds the source reads to DB-shaped files. An ordinary module
/// merely sitting under such a directory, with none of the markers, is not
/// recognised and stays flaggable.
fn is_typeorm_glob_loaded_file(path: &Path) -> bool {
    if !matches!(Language::from_path(path), Some(Language::TypeScript | Language::Tsx)) {
        return false;
    }
    if !has_typeorm_artifact_path_shape(path) {
        return false;
    }
    let Ok(source) = std::fs::read_to_string(path) else {
        return false;
    };
    source_has_typeorm_artifact_marker(&source)
}

/// Cheap pre-filter: the file's name or one of its directory segments follows
/// the conventional TypeORM entity/subscriber/migration layout (`*.entity.ts`,
/// or a path segment of `entity`/`entities`/`subscriber`/`subscribers`/
/// `migration`/`migrations`). Bounds [`is_typeorm_glob_loaded_file`]'s source
/// read to DB-shaped files instead of every indexed path.
fn has_typeorm_artifact_path_shape(path: &Path) -> bool {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if stem.ends_with(".entity")
        || stem.ends_with(".subscriber")
        || stem.ends_with(".migration")
    {
        return true;
    }
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("entity")
                | Some("entities")
                | Some("subscriber")
                | Some("subscribers")
                | Some("migration")
                | Some("migrations")
        )
    })
}

/// True when `source` carries a TypeORM-specific registration marker: an
/// `@Entity`/`@ViewEntity`/`@ChildEntity`/`@EventSubscriber` decorator call or
/// an `implements MigrationInterface` clause. These come from `typeorm` imports
/// and are the runtime hooks the DataSource glob loader keys on.
fn source_has_typeorm_artifact_marker(source: &str) -> bool {
    source.contains("@Entity(")
        || source.contains("@ViewEntity(")
        || source.contains("@ChildEntity(")
        || source.contains("@EventSubscriber(")
        || source.contains("implements MigrationInterface")
}

fn is_entry_point(
    path: &Path,
    project: &ProjectCtx,
    canon_root: Option<&Path>,
    canon_workspace_roots: &std::collections::HashSet<std::path::PathBuf>,
) -> bool {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    if is_rust_entry_point(path) {
        return true;
    }

    if is_config_file(path) {
        return true;
    }

    if is_framework_entry_point(path, project) {
        return true;
    }

    // Storybook story files (`*.stories.{ts,tsx,js,jsx,mjs,cjs,mdx}`, also the
    // older `*.story.*`) are discovered by Storybook's `stories: [...]` glob in
    // `.storybook/main.*` and loaded at runtime, never `import`ed — so the
    // import-graph BFS cannot reach them, yet they are real entry points.
    if is_storybook_story(path) {
        return true;
    }

    if is_angular_schematic_or_migration_factory(path, project) {
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

    // Files under a demonstration directory (`examples/`, `example/`, `demo/`,
    // `demos/`, `samples/`, `sample/`, …) are standalone runnable scripts and
    // documentation examples: they import the library but nothing imports them,
    // so they are leaf entry points, not dead code. This covers nested
    // `packages/<pkg>/examples/` dirs that the top-level-only
    // `is_top_level_script_dir_path` check misses.
    if is_sample_dir_path(path) {
        return true;
    }

    let Some(canon_root) = canon_root else {
        return false;
    };

    // CLIs and smoke/build tools are run directly, never imported. They live in
    // a top-level `scripts/`, `bin/`, `tools/`, `examples/`, `example-apps/`,
    // `benchmark/`, or `benchmarks/` directory.
    if is_top_level_script_dir_path(path, canon_root) {
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

/// True for an Angular schematic or `ng update` migration factory file: a
/// source file under a `schematics/`/`migrations/` directory of an Angular
/// package (one declaring `@angular/core` in its nearest `package.json`). The
/// Angular CLI loads these by path string from the `collection.json`/
/// `migration.json` manifest's `factory` field, never via a TypeScript
/// `import`, so the import-graph BFS cannot reach them — yet they are real
/// entry points. The Angular gate keeps a non-Angular project's `migrations/`
/// directory (database migration scripts) flaggable.
fn is_angular_schematic_or_migration_factory(path: &Path, project: &ProjectCtx) -> bool {
    if !is_angular_schematic_or_migration_entry(path) {
        return false;
    }
    project.frameworks_for_path(path).iter().any(|fw| fw.name == "angular")
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

/// True for a Cargo entry point that the import-graph BFS cannot reach on its
/// own. `lib.rs` / `main.rs` are crate roots Cargo compiles automatically — in
/// a workspace each member has its own, and a sibling crate reaches them only
/// via `use <crate_name>::…` (an external-crate name the module resolver does
/// not follow), so each must seed the reachable set. `build.rs` is a build
/// script Cargo executes directly; it is never `mod`-included, so nothing in the
/// import graph references it. Seeding the crate roots makes their whole module
/// tree (`mod.rs` and submodule files, via `mod` edges) reachable.
fn is_rust_entry_point(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("lib.rs") | Some("main.rs") | Some("build.rs")
    )
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
        || path_str.contains("/test/")
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

    // Regression for #1177: many projects (payloadcms/payload) place test
    // infrastructure — seeds, fixture collections, test-only Payload configs —
    // under a singular `test/` directory. These files are exclusively used during
    // testing and are unreachable from production entry points, but they are not
    // dead code. They must not be flagged.
    #[test]
    fn skips_files_under_singular_test_dir() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "export const app = 1;\n"),
            ("test/plugin-redirects/seed/index.ts", "export const seed = () => {};\n"),
            ("test/storage-r2/shared.ts", "export const shared = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "files under a singular test/ directory are test infrastructure, not dead code: {diags:?}"
        );
    }

    // Regression for #1177: the `test/` exemption is precise — an ordinary
    // orphaned source file under `src/` (no test path segment) is still a true
    // positive. Guards against the exemption blanketing genuine dead code.
    #[test]
    fn orphan_file_beside_test_dir_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "export const app = 1;\n"),
            ("test/plugin-redirects/seed/index.ts", "export const seed = () => {};\n"),
            ("src/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "only the genuine orphan must be flagged; test/ files are test infrastructure: {diags:?}"
        );
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

    // Regression for #1816: pnpm monorepos declare workspace members in
    // `pnpm-workspace.yaml` (no `workspaces` field in the root package.json) and
    // each member publishes its library as subpath exports pointing at non-`.`
    // index files (e.g. `@tiptap/pm` exposing `./inputrules` →
    // `./inputrules/index.ts`). Those subpath entry files, and everything
    // reachable from them, must be seeded. A genuinely orphaned file inside a
    // workspace package is still flagged.
    #[test]
    fn pnpm_workspace_subpath_export_entries_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n"),
            // Root package.json declares no `workspaces` field and is not a
            // library (no main/exports), so the rule's anchor (root index.ts)
            // does not short-circuit the run.
            ("package.json", r#"{"name":"tiptap","private":true}"#),
            ("index.ts", "export const root = 1;\n"),
            // @tiptap/pm: subpath export with no `.` target, pointing at a
            // nested index file.
            (
                "packages/pm/package.json",
                r#"{"name":"@tiptap/pm","exports":{"./inputrules":"./inputrules/index.ts"}}"#,
            ),
            (
                "packages/pm/inputrules/index.ts",
                "import { helper } from './helper';\nexport { helper };\n",
            ),
            ("packages/pm/inputrules/helper.ts", "export const helper = 1;\n"),
            // Genuine orphan inside a workspace package: not exported, not
            // imported by anything — must still be flagged.
            ("packages/pm/inputrules/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(
            diags.len(),
            1,
            "only the orphan must be flagged; subpath-export entries and their \
             imports must be seeded: {diags:?}"
        );
        assert!(
            diags[0].path.to_str().is_some_and(|p| p.contains("orphan")),
            "the flagged file must be the genuine orphan inside the workspace \
             package, not a subpath-export entry: {diags:?}"
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

    // Regression for #1951: files inside a `__mocks__/` directory are Jest/
    // Vitest manual mocks, auto-loaded by the test runner via a string-based
    // lookup when test code calls `jest.mock(...)`. Nothing `import`s them, so
    // the import graph cannot reach them — but they are test infrastructure
    // loaded by convention, not dead code, and must not be flagged.
    #[test]
    fn auto_mock_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "export const app = 1;\n"),
            ("__mocks__/react-native.js", "module.exports = {};\n"),
            ("__mocks__/react-primitives.js", "module.exports = {};\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "__mocks__/ files are auto-loaded test mocks, not dead code: {diags:?}"
        );
    }

    // Regression for #1951: the `__mocks__/` exemption is precise — a genuinely
    // orphaned ordinary source file is still a true positive even when the
    // project contains a `__mocks__/` directory.
    #[test]
    fn orphan_file_beside_auto_mocks_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "export const app = 1;\n"),
            ("__mocks__/react-native.js", "module.exports = {};\n"),
            ("src/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "only the genuine orphan must be flagged; the mock is test infra: {diags:?}"
        );
    }

    // Regression for #1951: the exemption matches `__mocks__` as a path segment,
    // not a substring — an orphaned `src/__mocks__data.ts` (no `__mocks__/`
    // directory boundary) is regular source and stays a true positive.
    #[test]
    fn auto_mock_substring_file_is_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "export const app = 1;\n"),
            ("src/__mocks__data.ts", "export const data = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("__mocks__data"),
            "a `__mocks__` substring (not a path segment) is not exempted: {diags:?}"
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

    // Regression for #1624: Angular schematic (`schematics/ng-add/index.ts`) and
    // `ng update` migration (`migrations/14_0_0/index.ts`) factory files are
    // loaded by the Angular CLI by path string from `collection.json` /
    // `migration.json`, never `import`ed, so the import-graph BFS cannot reach
    // them. They are framework entry points and must not be flagged. The root
    // package.json is a non-library (rule runs); the Angular module declares the
    // `schematics`/`ng-update` keys.
    #[test]
    fn angular_schematic_and_migration_entries_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"platform"}"#),
            ("src/index.ts", "export const root = 1;\n"),
            (
                "modules/router-store/package.json",
                r#"{"name":"@ngrx/router-store","dependencies":{"@angular/core":"^17.0.0"},"schematics":"./schematics/collection.json","ng-update":{"migrations":"./migrations/migration.json"}}"#,
            ),
            (
                "modules/router-store/schematics/ng-add/index.ts",
                "export default function(): unknown { return {}; }\n",
            ),
            (
                "modules/router-store/migrations/14_0_0/index.ts",
                "export default function(): unknown { return {}; }\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "Angular schematic/migration factory entry points must not be flagged: {diags:?}"
        );
    }

    // Regression for #1624: the Angular schematic/migration exemption is gated on
    // an Angular dependency in the nearest package.json — a project WITHOUT
    // `@angular/core` does not get a blanket pass for an orphaned file that merely
    // sits under a `schematics/` directory.
    #[test]
    fn schematics_dir_orphan_flagged_without_angular() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"app"}"#),
            ("src/index.ts", "export const app = 1;\n"),
            (
                "schematics/ng-add/index.ts",
                "export default function(): unknown { return {}; }\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("schematics"),
            "without Angular, an orphaned schematics/ file is still flagged: {diags:?}"
        );
    }

    // Regression for #1624: the exemption is precise — in an Angular project, a
    // genuinely orphaned ordinary source file (outside any schematics/migrations
    // directory) is still a true positive.
    #[test]
    fn orphan_file_in_angular_project_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"platform"}"#),
            ("src/index.ts", "export const root = 1;\n"),
            (
                "modules/router-store/package.json",
                r#"{"name":"@ngrx/router-store","dependencies":{"@angular/core":"^17.0.0"},"schematics":"./schematics/collection.json"}"#,
            ),
            (
                "modules/router-store/schematics/ng-add/index.ts",
                "export default function(): unknown { return {}; }\n",
            ),
            ("modules/router-store/src/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "only the genuine orphan must be flagged; schematic entries are entry points: {diags:?}"
        );
    }

    // Regression for #1359: Storybook story files (`*.stories.tsx`, etc.) are
    // discovered by Storybook's `stories: [...]` glob and loaded at runtime,
    // never `import`ed, so the import-graph BFS cannot reach them — but they are
    // central entry points, not dead code, and must not be flagged. (`.stories.mdx`
    // is also a story extension, covered in path_utils::is_storybook_story's unit
    // test; comply does not process `.mdx` as a source language, so an `.mdx` file
    // never enters the import index here.)
    #[test]
    fn storybook_story_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "export const app = 1;\n"),
            (
                "components/Button.stories.tsx",
                "import { Button } from './Button';\nexport default { component: Button };\n",
            ),
            (
                "components/button.stories.js",
                "export default { title: 'Button' };\n",
            ),
            ("components/Toggle.story.jsx", "export default { title: 'Toggle' };\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "Storybook story files are glob-loaded entry points, not dead code: {diags:?}"
        );
    }

    // Regression for #1359: the Storybook exemption is precise — a genuinely
    // orphaned ordinary source file is still a true positive even when the
    // project contains story files.
    #[test]
    fn orphan_file_beside_storybook_stories_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("src/index.ts", "export const app = 1;\n"),
            (
                "components/Button.stories.tsx",
                "export default { title: 'Button' };\n",
            ),
            ("src/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "only the genuine orphan must be flagged; the story is an entry point: {diags:?}"
        );
    }

    // Regression for #1214: files under a (possibly nested) demonstration
    // directory (`examples/`, `example/`, `demo/`, `samples/`, …) are standalone
    // runnable scripts / documentation examples — they import the library but
    // nothing imports them, so they are leaf entry points, not dead code. The
    // effect-ts cases live in nested `packages/<pkg>/examples/` dirs that the
    // top-level-only entry-dir check misses.
    #[test]
    fn nested_example_demo_sample_dir_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            (
                "packages/sql-mysql2/examples/statement-transform.ts",
                "import { Sql } from '@effect/sql';\nvoid Sql;\n",
            ),
            (
                "packages/opentelemetry/examples/index.ts",
                "import { Otel } from '@effect/opentelemetry';\nvoid Otel;\n",
            ),
            (
                "packages/sql-mssql/examples/migrations/0001_create_people.ts",
                "export default () => {};\n",
            ),
            ("packages/ui/components/tabs/demo/basic.tsx", "export const Demo = 1;\n"),
            ("packages/core/samples/usage.ts", "import { lib } from 'core';\nvoid lib;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "files under example/demo/sample dirs are standalone entry points: {diags:?}"
        );
    }

    // Regression for #1214: the example-dir exemption is precise — a genuinely
    // orphaned ordinary source file under `src/` (no example/demo/sample path
    // segment, imported by nothing) is still a true positive.
    #[test]
    fn orphan_file_outside_example_dirs_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            (
                "packages/sql-mysql2/examples/statement-transform.ts",
                "import { Sql } from '@effect/sql';\nvoid Sql;\n",
            ),
            ("src/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "only the genuine orphan must be flagged; example files are entry points: {diags:?}"
        );
    }

    // Regression for #1214: the exemption matches `examples` as a path segment,
    // not a substring — an orphaned `src/myexamples/foo.ts` (no `examples/`
    // directory boundary) is regular source and stays a true positive.
    #[test]
    fn example_substring_dir_file_is_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export const root = 1;\n"),
            ("src/myexamples/foo.ts", "export const foo = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("myexamples"),
            "a `examples` substring (not a path segment) is not exempted: {diags:?}"
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

    // Regression for #1271 (kanidm): a Rust workspace's secondary-crate entry
    // points are Cargo conventions the import-graph BFS cannot reach on its own.
    // The primary binary crate's `main.rs` is a BFS root, but it pulls the
    // secondary `proto` crate in via `use proto::…` — an external-crate name the
    // module resolver cannot follow to `proto/src/lib.rs`. So `proto`'s crate
    // root, its `build.rs` build script (never `mod`-included), and its
    // `mod`-included modules (`v1/mod.rs` and the `v1/message.rs` it declares)
    // are all unreachable from the binary, yet every one is a Cargo entry point
    // or reachable through `mod` and must not be flagged.
    #[test]
    fn rust_workspace_crate_entry_points_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("Cargo.toml", "[workspace]\nmembers = [\"app\", \"proto\"]\n"),
            ("app/Cargo.toml", "[package]\nname = \"app\"\n"),
            ("app/src/main.rs", "use proto::v1::message::Message;\nfn main() {}\n"),
            ("proto/Cargo.toml", "[package]\nname = \"proto\"\n"),
            ("proto/build.rs", "fn main() {}\n"),
            ("proto/src/lib.rs", "pub mod v1;\n"),
            ("proto/src/v1/mod.rs", "pub mod message;\n"),
            ("proto/src/v1/message.rs", "pub struct Message;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "lib.rs (crate root), build.rs (build script), and mod-included \
             v1/mod.rs + v1/message.rs are all Cargo entry points / reachable: {diags:?}"
        );
    }

    // Regression for #1271: the Cargo entry-point recognition is precise — a
    // genuinely orphaned `.rs` file that is neither a crate root, nor a build
    // script, nor declared by any `mod <name>;` is still a true positive.
    #[test]
    fn orphan_rust_file_not_mod_included_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("Cargo.toml", "[workspace]\nmembers = [\"app\", \"proto\"]\n"),
            ("app/Cargo.toml", "[package]\nname = \"app\"\n"),
            ("app/src/main.rs", "fn main() {}\n"),
            ("proto/Cargo.toml", "[package]\nname = \"proto\"\n"),
            ("proto/src/lib.rs", "pub mod v1;\n"),
            ("proto/src/v1/mod.rs", "pub struct V1;\n"),
            ("proto/src/dead.rs", "pub struct Dead;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("dead.rs"),
            "only the genuinely orphaned dead.rs (declared by no `mod`) is flagged: {diags:?}"
        );
    }

    // Regression for #2374 (typeorm): a monorepo's root package is a published
    // library (`main`/`exports` → `is_library=true`), but the rule's anchor
    // (lexicographically smallest path) falls in a non-library sub-package
    // (`docs/`, no `main`/`exports`), so the per-anchor `is_library` short-circuit
    // does not fire and the run proceeds. The library's source entry barrel
    // (`src/index.ts`, whose stem matches the package's `main`/`exports` entry
    // stem) is seeded as a BFS root, so every decorator file re-exported from it
    // via `export *` stays reachable instead of being flagged.
    #[test]
    fn library_source_not_flagged_when_anchor_is_docs_subpackage() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"typeorm","main":"./index.js","exports":{".":"./index.js"}}"#,
            ),
            // src/index.ts is the library's source entry barrel: its stem
            // matches the package's `main`/`exports` entry stem, so it must be
            // seeded as a BFS root even when the run anchors elsewhere.
            (
                "src/index.ts",
                "export * from './decorator/columns/VirtualColumn';\n\
                 export * from './decorator/listeners/AfterInsert';\n\
                 export * from './decorator/ForeignKey';\n",
            ),
            (
                "src/decorator/columns/VirtualColumn.ts",
                "export function VirtualColumn(): unknown { return {}; }\n",
            ),
            (
                "src/decorator/listeners/AfterInsert.ts",
                "export function AfterInsert(): unknown { return {}; }\n",
            ),
            (
                "src/decorator/ForeignKey.ts",
                "export function ForeignKey(): unknown { return {}; }\n",
            ),
            // docs/ is a non-library sub-package; its sidebars.ts is the
            // lexicographically smallest indexed path, so it becomes the anchor.
            ("docs/package.json", r#"{"name":"docs","private":true}"#),
            ("docs/sidebars.ts", "export default { sidebar: [] };\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        let flagged: Vec<&str> = diags.iter().filter_map(|d| d.path.to_str()).collect();
        assert!(
            !flagged.iter().any(|p| p.contains("/src/decorator/")),
            "library decorator files re-exported from src/index.ts must not be \
             flagged when the anchor is a non-library docs sub-package: {flagged:?}"
        );
    }

    // Regression for #2374: the entry-barrel seeding is precise — it only makes
    // files reachable *from* a declared entry barrel reachable. A genuinely
    // unreferenced source file in a NON-library sub-package (no inbound import
    // edge, not an entry, not reachable from any barrel) is still flagged, so
    // real dead code is not blanketed by the docs-anchor fix.
    #[test]
    fn genuinely_unreferenced_file_in_non_library_package_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"typeorm","main":"./index.js","exports":{".":"./index.js"}}"#,
            ),
            (
                "src/index.ts",
                "export * from './decorator/columns/VirtualColumn';\n",
            ),
            (
                "src/decorator/columns/VirtualColumn.ts",
                "export function VirtualColumn(): unknown { return {}; }\n",
            ),
            // app/ is a non-library sub-package (no main/exports). app/main.ts
            // is its bootstrapper entry and imports used.ts; orphan.ts is
            // imported by nothing and is not an entry — genuine dead code that
            // must stay flagged because app declares no published surface.
            ("app/package.json", r#"{"name":"app","private":true}"#),
            ("app/main.ts", "import { used } from './used';\nused();\n"),
            ("app/used.ts", "export const used = () => {};\n"),
            ("app/orphan.ts", "export const orphan = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("orphan"),
            "the genuinely unreferenced non-library file must still be flagged: {diags:?}"
        );
    }

    // Regression for #2311 (typeorm): entity, subscriber, and migration files are
    // registered at runtime via the DataSource `entities`/`subscribers`/
    // `migrations` glob patterns, never `import`ed from an entry point, so the
    // import-graph BFS cannot reach them. The TypeORM-specific decorators
    // (`@Entity`, `@EventSubscriber`) and the `MigrationInterface` implementation
    // are strong evidence the file is glob-loaded; such files (and everything they
    // import) must not be flagged.
    #[test]
    fn typeorm_glob_loaded_files_are_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"app"}"#),
            // Real entry point so the rule runs. It boots the DataSource, which
            // registers entities/subscribers/migrations via runtime glob strings
            // — there is no static import edge to any of those files.
            (
                "src/index.ts",
                "import { ds } from './data-source';\nvoid ds;\n",
            ),
            (
                "src/data-source.ts",
                "import { DataSource } from 'typeorm';\n\
                 export const ds = new DataSource({\n\
                 \x20\x20entities: [__dirname + '/entity/*.ts'],\n\
                 \x20\x20subscribers: [__dirname + '/subscriber/*.ts'],\n\
                 \x20\x20migrations: [__dirname + '/migration/*.ts'],\n\
                 });\n",
            ),
            // Entity referenced only via the entities glob. It imports a sibling
            // entity and a non-decorated column helper — both must stay reachable.
            (
                "src/entity/Category.ts",
                "import { Entity, PrimaryColumn, ManyToMany } from 'typeorm';\n\
                 import { Post } from './Post';\n\
                 import { slugify } from './slug';\n\
                 @Entity()\n\
                 export class Category {\n\
                 \x20\x20@PrimaryColumn()\n\
                 \x20\x20id!: string;\n\
                 \x20\x20@ManyToMany(() => Post)\n\
                 \x20\x20posts!: Post[];\n\
                 \x20\x20slug = slugify(this.id);\n\
                 }\n",
            ),
            (
                "src/entity/Post.ts",
                "import { Entity, PrimaryColumn } from 'typeorm';\n\
                 @Entity()\n\
                 export class Post { @PrimaryColumn() id!: string; }\n",
            ),
            // Non-decorated helper reachable only through the glob-loaded entity.
            ("src/entity/slug.ts", "export const slugify = (s: string) => s;\n"),
            (
                "src/subscriber/PostSubscriber.ts",
                "import { EventSubscriber, EntitySubscriberInterface } from 'typeorm';\n\
                 @EventSubscriber()\n\
                 export class PostSubscriber implements EntitySubscriberInterface {}\n",
            ),
            (
                "src/migration/1700000000000-Init.ts",
                "import { MigrationInterface, QueryRunner } from 'typeorm';\n\
                 export class Init1700000000000 implements MigrationInterface {\n\
                 \x20\x20async up(q: QueryRunner): Promise<void> {}\n\
                 \x20\x20async down(q: QueryRunner): Promise<void> {}\n\
                 }\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files);
        let flagged: Vec<&str> = diags.iter().filter_map(|d| d.path.to_str()).collect();
        assert!(
            flagged.is_empty(),
            "TypeORM glob-loaded entity/subscriber/migration files and their \
             transitive imports must not be flagged: {flagged:?}"
        );
    }

    // Regression for #2311: the TypeORM exemption is gated on the file content
    // signal, not the directory name alone. An ordinary orphaned source file under
    // an `entity/` directory that carries no `@Entity`/`@EventSubscriber`/
    // `MigrationInterface` signal is still a true positive.
    #[test]
    fn orphan_in_entity_dir_without_typeorm_signal_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"app"}"#),
            ("src/index.ts", "export const app = 1;\n"),
            // Lives under entity/ but is a plain module — no decorator/interface.
            ("src/entity/helpers.ts", "export const helper = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("helpers"),
            "a plain module under entity/ with no TypeORM signal is still flagged: {diags:?}"
        );
    }

    // Regression for #2373 (express 5.x): a published library declares a `files`
    // whitelist but no `main`/`exports`/`module`, relying on npm's default
    // `index.js` entry resolution. `is_library` is false (so the rule runs), but
    // the files inside the published `files` surface are reachable only through
    // the package boundary, never `import`ed within the repo. They must not be
    // flagged.
    #[test]
    fn published_files_surface_without_main_exports_not_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"express","files":["LICENSE","Readme.md","index.js","lib/"]}"#,
            ),
            ("index.js", "module.exports = require('./lib/express');\n"),
            ("lib/express.js", "module.exports = function express() {};\n"),
            ("lib/router.js", "module.exports = function router() {};\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert!(
            diags.is_empty(),
            "files within the published `files` surface of a library with no \
             main/exports must not be flagged: {diags:?}"
        );
    }

    // Regression for #2373: the `files`-surface exemption is precise — a source
    // file that is neither imported nor part of the published `files` whitelist
    // (here `internal/scratch.js`, outside the published `lib/`) is still a true
    // positive.
    #[test]
    fn orphan_outside_published_files_surface_still_flagged() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"express","files":["index.js","lib/"]}"#,
            ),
            ("index.js", "module.exports = require('./lib/express');\n"),
            ("lib/express.js", "module.exports = function express() {};\n"),
            ("internal/scratch.js", "module.exports = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files);
        assert_eq!(diags.len(), 1, "expected one unused-file diagnostic: {diags:?}");
        assert!(
            diags[0].path.to_str().unwrap().contains("scratch"),
            "only the orphan outside the published `files` surface is flagged: {diags:?}"
        );
    }
}
