//! Shared path classifiers used by multiple rules.
//!
//! Centralised so `unused-file` and `dead-export` agree on what counts as a
//! config file and don't drift apart over time.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;

use crate::project::ProjectCtx;

thread_local! {
    /// Per-thread memo of `canonicalize`. The project root and each file's
    /// parent directory are canonicalized once per classifier call; the same
    /// directories recur across thousands of files in a run, and the project
    /// root is constant. canonicalize hits the filesystem (one syscall per
    /// path segment), so memoizing collapses the bulk of those syscalls.
    /// Results are deterministic for the duration of a run, so the memo is
    /// output-identical to calling `canonicalize` directly.
    static CANON_CACHE: RefCell<FxHashMap<PathBuf, PathBuf>> =
        RefCell::new(FxHashMap::default());
}

/// `std::fs::canonicalize(p)` with a per-thread memo, falling back to `p`
/// itself on error (same as the previous inline `unwrap_or_else`).
fn canonicalize_cached(p: &Path) -> PathBuf {
    CANON_CACHE.with(|c| {
        if let Some(v) = c.borrow().get(p) {
            return v.clone();
        }
        let v = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        c.borrow_mut().insert(p.to_path_buf(), v.clone());
        v
    })
}

/// True if `path` is a build/tooling config file. Matches `*.config.*`
/// (e.g. `vite.config.ts`, `jest.config.js`), the Vitest `*.workspace.*`
/// convention (e.g. `vitest.workspace.ts`, loaded by filename and never
/// imported), and dotfile-rc entries (e.g. `.eslintrc.js`, `.babelrc.ts`).
pub fn is_config_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if stem.ends_with(".config") || stem.ends_with(".workspace") {
        return true;
    }
    if name.starts_with('.') && stem.ends_with("rc") {
        return true;
    }
    false
}

/// True when `path` lives inside a jscodeshift codemod fixture directory: an
/// ancestor directory whose name ends in `.test` (e.g.
/// `menu-item-primary-text.test/actual.js`). These directories hold the
/// pre-/post-transformation snippets a codemod operates on; their JSX
/// references components without importing them on purpose, so identifier-
/// resolution rules must not lint them.
pub fn is_codemod_fixture_file(path: &Path) -> bool {
    path.parent().is_some_and(|parent| {
        parent.components().any(|c| {
            c.as_os_str()
                .to_str()
                .is_some_and(|seg| seg.ends_with(".test"))
        })
    })
}

/// True when any `Normal` path segment equals one of `names`. Segment (not
/// substring) matching is what keeps `src/appconfig/` from matching `config`
/// and `src/mysamples/` from matching `samples`.
fn has_path_segment(path: &Path, names: &[&str]) -> bool {
    path.components().any(|c| {
        matches!(c, std::path::Component::Normal(s) if s.to_str().is_some_and(|seg| names.contains(&seg)))
    })
}

/// Directory names where dev-only tooling lives and rules that exempt
/// "not shipped in the published package" code (e.g. `no-extraneous-import`)
/// are relaxed. The broad set: build/CI scripts, demonstration code,
/// generator scaffold templates, and performance benchmark suites. Matched as
/// exact path segments.
const AUX_DIR_SEGMENTS: &[&str] = &[
    "scripts",
    "bin",
    "config",
    "migrations",
    "samples",
    "samples-dev",
    "examples",
    "example-apps",
    "templates",
    "template",
    "scaffold",
    "boilerplate",
    "benchmarks",
    "bench",
    "perf",
    "perf-testing",
    "performance",
    "performance-tests",
    "__performance_tests__",
];

/// True when `path` lives under any auxiliary (non-shipped) directory: build
/// scripts, config/migrations, demonstration samples/examples, or generator
/// scaffold templates. The broad superset consumed by `no-extraneous-import`;
/// narrower rules use [`is_developer_script_path`] instead. Segment match.
pub fn is_aux_dir_path(path: &Path) -> bool {
    has_path_segment(path, AUX_DIR_SEGMENTS)
}

/// True when any path segment is cargo-fuzz's `fuzz_targets/` directory. In a
/// libfuzzer-sys target, `panic!` is the deliberate crash-signaling mechanism
/// the fuzzer catches, so panic/abort rules exempt these files.
pub fn is_fuzz_targets_path(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "fuzz_targets")
}

/// True for files under a developer-only directory (`scripts/`, `bin/`,
/// `migrations/`). One-off data-processing and migration scripts trade
/// readability for getting the job done; this is the narrow subset of
/// [`is_aux_dir_path`] used where exempting config/examples would be wrong
/// (e.g. `nested-control-flow`). Segment match.
pub fn is_developer_script_path(path: &Path) -> bool {
    has_path_segment(path, &["scripts", "bin", "migrations"])
}

/// True for build/codegen scripts under a `scripts/` or `config/` directory.
/// These run at dev/CI time and are not part of the shipped bundle.
pub fn is_build_script_path(path: &Path) -> bool {
    has_path_segment(path, &["scripts", "config"])
}

/// True for demonstration code under `samples/`, `samples-dev/`, `examples/`,
/// or `example-apps/`. Compiled and run at dev time to show library usage; it
/// never ships in the published package.
pub fn is_sample_dir_path(path: &Path) -> bool {
    has_path_segment(path, &["samples", "samples-dev", "examples", "example-apps"])
}

/// True for source files housed in a scaffold CLI's template directory
/// (`template/`, `templates/`, `scaffold/`, `boilerplate/`). A scaffold tool
/// (create-react-app, create-t3-app, `create-vite`, …) assembles these template
/// files — drawn from different subdirectories — into the generated project,
/// where cross-file relative imports resolve. Before assembly, those imports
/// point at not-yet-colocated siblings, so path-resolution rules must skip them.
/// Narrower than [`is_aux_dir_path`] on purpose: it omits `config`/`scripts`/
/// `migrations`/`examples`, where an unresolved relative import is still a real
/// error. Segment match.
pub fn is_scaffold_template_path(path: &Path) -> bool {
    has_path_segment(path, &["template", "templates", "scaffold", "boilerplate"])
}

/// True for files under a static-asset directory (`public/`, `static/`,
/// `assets/`). These are the conventional locations for files served verbatim
/// by the web server — vanilla `<script>`-loaded browser scripts, not ES
/// modules. A bundler never processes them, so module-only concerns such as
/// tree-shaking do not apply. Segment match keeps `src/publicApi/` and
/// `src/assetsLoader/` from matching.
pub fn is_browser_asset_dir_path(path: &Path) -> bool {
    has_path_segment(path, &["public", "static", "assets"])
}

/// True when `path` lives under a `__mocks__/` directory, the Jest/Vitest
/// manual-mock convention. These files are auto-loaded by the test runner to
/// replace a module during tests; they import the package they mock and other
/// test-only dependencies, and never ship in the published package. Segment
/// match keeps an unrelated `src/my__mocks__data/` from matching.
pub fn is_auto_mock_dir_path(path: &Path) -> bool {
    has_path_segment(path, &["__mocks__"])
}

/// True for a test file that never ships in the published package: one under a
/// `__tests__/`, `__testUtils__/`, `test/`, `tests/`, or `e2e/` directory, one
/// carrying a `.test.`/`.spec.`/`.setup.`/`.tp.` infix, one whose whole stem is
/// `test`/`spec` (a co-located `endOfWeek/test.ts`), or one whose name starts
/// with a test-runner tooling prefix (`vitest-`/`jest-`, e.g.
/// `vitest-custom-reporter.ts`). Consumed by `no-extraneous-import` to allow
/// test-only devDependency imports.
pub fn is_extraneous_test_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let is_marked = path_str.contains("__tests__")
        || path_str.contains("__testUtils__")
        || path_str.contains(".test.")
        || path_str.contains(".spec.")
        || path_str.contains(".setup.")
        || path_str.contains("/test/")
        || path_str.contains("/tests/")
        || path_str.contains("/e2e/");
    is_marked
        || has_test_file_stem(path)
        || has_type_probe_infix(path)
        || has_test_framework_tooling_prefix(path)
}

/// True when the file name starts with a known test-framework tooling prefix
/// (`vitest-`/`jest-`, e.g. `vitest-custom-reporter.ts`, `jest-setup.ts`).
/// Such files are test-runner infrastructure (custom reporters, environments,
/// setup) consumed only by the test runner and never shipped in the published
/// package, so importing a devDependency from them is correct.
fn has_test_framework_tooling_prefix(path: &Path) -> bool {
    const TEST_FRAMEWORK_PREFIXES: &[&str] = &["vitest-", "jest-"];
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            TEST_FRAMEWORK_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
}

/// Co-located test files whose entire name (minus extension) is `test` or
/// `spec` — e.g. `src/endOfWeek/test.ts`.
fn has_test_file_stem(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs")
    ) && (stem == "test" || stem == "spec")
}

/// True when the file name carries a `.test-d.` infix (e.g.
/// `Component.test-d.tsx`), the tsd type-testing convention for files that
/// live beside their source rather than in a `test-d/` directory.
pub fn has_test_d_infix(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().contains(".test-d."))
}

/// True when the file name carries a `.tp.` infix (e.g. `test.tp.ts`), the
/// date-fns type-probe convention. Type-probe files exist solely to assert that
/// the public API type-checks; they are never shipped or run as runtime code, so
/// they are test files like the tsd `.test-d.` convention.
pub fn has_type_probe_infix(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().contains(".tp."))
}

/// True for an import specifier that traverses into a build-output or
/// code-generated directory (`dist`/`build`/`out` bundles, `generated`/
/// `__generated__`/`.prisma`/`prisma`/`gen` codegen output) or `node_modules`.
/// These artifacts are produced by a build step, gitignored, and absent in a
/// clean checkout, so an unresolved import into them is expected. Segment match
/// over the `/`-split specifier (so `./distance` is NOT a `dist` match).
pub fn is_build_output_specifier(spec: &str) -> bool {
    spec.split('/').any(|seg| {
        matches!(
            seg,
            "dist"
                | "build"
                | "out"
                | "generated"
                | "__generated__"
                | ".prisma"
                | "prisma"
                | "gen"
                | "node_modules"
        )
    })
}

/// True for an import specifier pointing at a build-time generated file: a
/// final segment ending in `.gen` (e.g. `./routeTree.gen`), a `.gen.` extension
/// stem (e.g. `./routeTree.gen.ts`), or a `.prebuilt.` extension stem (e.g.
/// `./idle.prebuilt.js`). Such files are gitignored and often absent at lint
/// time, yet always present at build/dev time.
pub fn is_generated_file_specifier(spec: &str) -> bool {
    let last = spec.rsplit('/').next().unwrap_or(spec);
    last.ends_with(".gen") || last.contains(".gen.") || last.contains(".prebuilt.")
}

/// True when `path` matches a framework entry point via FILES, SUFFIXES, or
/// ROOT_FILES only — does NOT check dirs. Used by `dead-export` to bail out
/// for framework-specific files even when the user has configured additional
/// entrypoints (which disables the dirs bail-out).
pub fn is_framework_specific_entry_point(path: &Path, project: &ProjectCtx) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if project.framework_entry_files().any(|entry| entry == name) {
        return true;
    }
    if project
        .framework_entry_file_suffixes()
        .any(|suffix| name.ends_with(suffix))
    {
        return true;
    }

    let Some(root) = project.project_root.as_deref() else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let canon_parent = canonicalize_cached(parent);
    let canon_root = canonicalize_cached(root);
    if canon_parent != canon_root {
        return false;
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    project.framework_root_files().any(|entry| entry == stem)
}

/// True when `path` lives under a framework entry_points.dirs directory.
/// Used by `dead-export` to suppress the dirs bail-out only when the user
/// has NOT configured additional entrypoints (backward-compat mode).
pub fn is_in_framework_entry_dir(path: &Path, project: &ProjectCtx) -> bool {
    let path_str = path.to_string_lossy().replace('\\', "/");
    project
        .framework_entry_dirs()
        .any(|dir| path_str.contains(dir))
}

/// True when `file_name` is a SvelteKit route file: a `+`-prefixed basename
/// from the framework's file-system routing set (`+page`, `+layout`,
/// `+server`, `+error` with `.svelte`/`.ts`/`.js` and an optional `.server`
/// segment). These are discovered by the router at build time, never imported.
pub fn is_sveltekit_route_file(file_name: &str) -> bool {
    let Some(rest) = file_name.strip_prefix('+') else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    matches!(
        parts.as_slice(),
        ["page" | "layout" | "error", "svelte"]
            | ["page" | "layout", "js" | "ts"]
            | ["page" | "layout", "server", "js" | "ts"]
            | ["server", "js" | "ts"]
    )
}

/// True when `path` is a SvelteKit route file (`+page.svelte`,
/// `+page.server.ts`, `+server.ts`, …) located under a `routes/` directory in
/// a project where SvelteKit is detected. SvelteKit's file-system router
/// consumes these by path, so nothing imports them — they are implicit entry
/// points. The `routes/` ancestor and detection gate keep the exemption from
/// covering an unrelated `+`-named file.
fn is_sveltekit_route_entry(path: &Path, project: &ProjectCtx) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !is_sveltekit_route_file(name) {
        return false;
    }
    if !path
        .components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("routes"))
    {
        return false;
    }
    project.has_framework("svelte")
        || project.frameworks_for_path(path).iter().any(|f| f.name == "svelte")
}

/// True when `path` matches an entry point declared by any detected
/// framework. This covers file-based routers, generated route trees, and
/// framework-owned files whose exports/import reachability is implicit.
pub fn is_framework_entry_point(path: &Path, project: &ProjectCtx) -> bool {
    if is_sveltekit_route_entry(path, project) {
        return true;
    }

    let path_str = path.to_string_lossy().replace('\\', "/");
    if project
        .framework_entry_dirs()
        .any(|dir| path_str.contains(dir))
    {
        return true;
    }

    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if project.framework_entry_files().any(|entry| entry == name) {
        return true;
    }
    if project
        .framework_entry_file_suffixes()
        .any(|suffix| name.ends_with(suffix))
    {
        return true;
    }

    // Fall back to the framework owning this file via its nearest package.json:
    // a framework app nested in a subdirectory (a Next.js example under a
    // library's `app/`, a monorepo package) is invisible to the root-anchored
    // `detected_frameworks`. Its `dirs`/`files`/`suffixes` are path-relative,
    // so they identify file-system-routed entry points (Next.js `pages/`,
    // Remix `routes/`, SvelteKit `src/routes/`) regardless of detection depth.
    for fw in project.frameworks_for_path(path) {
        if fw.entry_points.dirs.iter().any(|dir| path_str.contains(dir.as_str())) {
            return true;
        }
        if fw.entry_points.files.iter().any(|entry| entry == name) {
            return true;
        }
        if fw
            .entry_points
            .file_suffixes
            .iter()
            .any(|suffix| name.ends_with(suffix.as_str()))
        {
            return true;
        }
    }

    let Some(root) = project.project_root.as_deref() else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let canon_parent = canonicalize_cached(parent);
    let canon_root = canonicalize_cached(root);
    if canon_parent != canon_root {
        return false;
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    project.framework_root_files().any(|entry| entry == stem)
}

#[cfg(test)]
mod aux_path_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn aux_dir_segments_match() {
        for dir in [
            "scripts",
            "bin",
            "config",
            "migrations",
            "samples",
            "samples-dev",
            "examples",
            "example-apps",
            "templates",
            "template",
            "scaffold",
            "boilerplate",
            "benchmarks",
            "bench",
            "perf",
            "perf-testing",
            "performance",
            "performance-tests",
            "__performance_tests__",
        ] {
            assert!(
                is_aux_dir_path(&PathBuf::from(format!("pkg/{dir}/file.ts"))),
                "{dir}/ should be an aux dir"
            );
        }
        // Segment (not substring) match — guards no-extraneous-import's
        // still_flags_dev_dep_outside_config_dir / _sample_dirs tests.
        assert!(!is_aux_dir_path(&PathBuf::from("src/appconfig/index.ts")));
        assert!(!is_aux_dir_path(&PathBuf::from("src/mysamples/index.ts")));
        assert!(!is_aux_dir_path(&PathBuf::from("src/templated/index.ts")));
        assert!(!is_aux_dir_path(&PathBuf::from("src/app/login.ts")));
    }

    #[test]
    fn developer_script_path_is_narrow() {
        assert!(is_developer_script_path(&PathBuf::from("scripts/import-legacy-data.ts")));
        assert!(is_developer_script_path(&PathBuf::from("pkg/bin/cli.ts")));
        assert!(is_developer_script_path(&PathBuf::from("db/migrations/0001.ts")));
        // The narrow predicate must NOT cover config/examples/templates.
        assert!(!is_developer_script_path(&PathBuf::from("config/helpers.ts")));
        assert!(!is_developer_script_path(&PathBuf::from("examples/app/page.tsx")));
        assert!(!is_developer_script_path(&PathBuf::from("templates/app/page.tsx")));
    }

    #[test]
    fn build_script_and_sample_dir_segments() {
        assert!(is_build_script_path(&PathBuf::from("scripts/gen.ts")));
        assert!(is_build_script_path(&PathBuf::from("config/helpers.ts")));
        assert!(!is_build_script_path(&PathBuf::from("src/appconfig/index.ts")));
        assert!(is_sample_dir_path(&PathBuf::from("samples-dev/x.ts")));
        assert!(is_sample_dir_path(&PathBuf::from("examples/app.ts")));
        assert!(!is_sample_dir_path(&PathBuf::from("src/mysamples/index.ts")));
    }

    #[test]
    fn scaffold_template_path_is_narrow() {
        // Scaffold template dirs (issue #1753): create-t3-app layout.
        assert!(is_scaffold_template_path(&PathBuf::from(
            "cli/template/extras/src/app/page/base.tsx"
        )));
        assert!(is_scaffold_template_path(&PathBuf::from("templates/app/page.tsx")));
        assert!(is_scaffold_template_path(&PathBuf::from("scaffold/index.ts")));
        assert!(is_scaffold_template_path(&PathBuf::from("boilerplate/main.ts")));
        // Narrower than is_aux_dir_path: these must NOT be skipped — an
        // unresolved relative import in config/scripts/migrations/examples is a
        // real error.
        assert!(!is_scaffold_template_path(&PathBuf::from("config/next.config.js")));
        assert!(!is_scaffold_template_path(&PathBuf::from("scripts/gen.ts")));
        assert!(!is_scaffold_template_path(&PathBuf::from("examples/app/page.tsx")));
        // Segment (not substring) match.
        assert!(!is_scaffold_template_path(&PathBuf::from("src/templated/index.ts")));
        assert!(!is_scaffold_template_path(&PathBuf::from("src/app/login.ts")));
    }

    #[test]
    fn auto_mock_dir_path_segments() {
        // Issue #1755: Jest/Vitest `__mocks__/` manual-mock convention.
        assert!(is_auto_mock_dir_path(&PathBuf::from(
            "apps/react-vite/__mocks__/zustand.ts"
        )));
        assert!(is_auto_mock_dir_path(&PathBuf::from("__mocks__/fs.js")));
        // Segment (not substring) match.
        assert!(!is_auto_mock_dir_path(&PathBuf::from("src/my__mocks__data/index.ts")));
        assert!(!is_auto_mock_dir_path(&PathBuf::from("src/app/login.ts")));
    }

    #[test]
    fn fuzz_targets_path_match() {
        assert!(is_fuzz_targets_path(&PathBuf::from("fuzz/fuzz_targets/parse.rs")));
        assert!(!is_fuzz_targets_path(&PathBuf::from("src/lib.rs")));
    }

    #[test]
    fn extraneous_test_file_union() {
        assert!(is_extraneous_test_file(&PathBuf::from("src/login-form.test.tsx")));
        assert!(is_extraneous_test_file(&PathBuf::from("src/foo.spec.ts")));
        assert!(is_extraneous_test_file(&PathBuf::from("src/__testUtils__/expectJSON.ts")));
        assert!(is_extraneous_test_file(&PathBuf::from("src/endOfWeek/test.ts")));
        assert!(is_extraneous_test_file(&PathBuf::from("src/startOfWeek/spec.ts")));
        // Issue #1915: date-fns `.tp.` type-probe convention.
        assert!(is_extraneous_test_file(&PathBuf::from("src/addBusinessDays/test.tp.ts")));
        // Issue #1891: root-level test-runner tooling named with a framework prefix.
        assert!(is_extraneous_test_file(&PathBuf::from("vitest-custom-reporter.ts")));
        assert!(is_extraneous_test_file(&PathBuf::from("jest-setup.ts")));
        assert!(!is_extraneous_test_file(&PathBuf::from("src/app/login.ts")));
        // Guard: the prefix must be a real `vitest-`/`jest-` name boundary, not a
        // substring of an unrelated production file name.
        assert!(!is_extraneous_test_file(&PathBuf::from("src/jester.ts")));
    }

    #[test]
    fn test_d_infix_filename() {
        assert!(has_test_d_infix(&PathBuf::from("src/Component.test-d.tsx")));
        assert!(has_test_d_infix(&PathBuf::from("schema.test-d.ts")));
        // A `test-d/` directory without the filename infix is NOT a filename
        // match (the directory branch lives in file_ctx::scan_path).
        assert!(!has_test_d_infix(&PathBuf::from("test-d/schema.ts")));
        assert!(!has_test_d_infix(&PathBuf::from("src/Component.tsx")));
    }

    #[test]
    fn type_probe_infix_filename() {
        // Issue #1915: date-fns `test.tp.ts` type-probe convention (stem
        // `test.tp`, so the `.tp.` infix is the marker, not the stem).
        assert!(has_type_probe_infix(&PathBuf::from("src/addBusinessDays/test.tp.ts")));
        assert!(has_type_probe_infix(&PathBuf::from("src/addDays/foo.tp.tsx")));
        // A plain co-located source file is not a type probe.
        assert!(!has_type_probe_infix(&PathBuf::from("src/addDays/index.ts")));
    }

    #[test]
    fn build_output_specifier_segments() {
        // Build artifacts (issue #1005) and codegen dirs (issue #1420).
        assert!(is_build_output_specifier("../../../dist/cjs/index.js"));
        assert!(is_build_output_specifier("../build/index.js"));
        assert!(is_build_output_specifier("./out/index.js"));
        assert!(is_build_output_specifier("./generated/prisma/client"));
        assert!(is_build_output_specifier("../src/__generated__/graphql"));
        assert!(is_build_output_specifier("./.prisma/client"));
        assert!(is_build_output_specifier("./prisma/client"));
        assert!(is_build_output_specifier("./gen/api"));
        assert!(is_build_output_specifier("./node_modules/@prisma/client"));
        // Segment match — substrings of another segment still flag.
        assert!(!is_build_output_specifier("./src/index.js"));
        assert!(!is_build_output_specifier("../distance/index.js"));
        assert!(!is_build_output_specifier("./distribution/x"));
        assert!(!is_build_output_specifier("./lib/util.js"));
        assert!(!is_build_output_specifier("./generated-things"));
        assert!(!is_build_output_specifier("./does-not-exist"));
    }

    #[test]
    fn generated_file_specifier_suffixes() {
        assert!(is_generated_file_specifier("./routeTree.gen"));
        assert!(is_generated_file_specifier("./routeTree.gen.ts"));
        assert!(is_generated_file_specifier("../app/routeTree.gen"));
        assert!(is_generated_file_specifier("../../runtime/client/idle.prebuilt.js"));
        assert!(is_generated_file_specifier("./visible.prebuilt.js"));
        assert!(!is_generated_file_specifier("./routeTree"));
        assert!(!is_generated_file_specifier("./generated"));
        assert!(!is_generated_file_specifier("./idle.js"));
        assert!(!is_generated_file_specifier("./prebuilt"));
    }
}
