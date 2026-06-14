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
/// imported), dotfile-rc entries (e.g. `.eslintrc.js`, `.babelrc.ts`), and the
/// extensionless Knip config name (`knip.ts`/`knip.js`) — the Knip tool reads
/// its config by filename and never `import`s it, so its `default` export has
/// no static importer. `knip.config.*` is already covered by the `*.config.*`
/// branch.
pub fn is_config_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if stem.ends_with(".config") || stem.ends_with(".workspace") {
        return true;
    }
    if name.starts_with('.') && stem.ends_with("rc") {
        return true;
    }
    if matches!(name, "knip.ts" | "knip.js") {
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

/// True when `path` lives under an Angular `schematics/` or `migrations/`
/// directory. These hold Angular CLI schematic and `ng update` migration entry
/// points: each is an `index.ts` exporting a default factory function that the
/// CLI loads dynamically via the `collection.json`/`migration.json` manifest,
/// never imported from TypeScript. Despite being named `index.ts`, they are
/// executable entry points, not re-export barrels, and their factory bodies are
/// expected side effects — so the barrel-side-effects check skips them. Segment
/// match keeps an unrelated `src/migrationsHelper.ts` from matching.
pub fn is_angular_schematic_or_migration_entry(path: &Path) -> bool {
    has_path_segment(path, &["schematics", "migrations"])
}

/// True for files under a developer-only directory (`scripts/`, `bin/`,
/// `migrations/`). One-off data-processing and migration scripts trade
/// readability for getting the job done; this is the narrow subset of
/// [`is_aux_dir_path`] used where exempting config/examples would be wrong
/// (e.g. `nested-control-flow`). Segment match.
pub fn is_developer_script_path(path: &Path) -> bool {
    has_path_segment(path, &["scripts", "bin", "migrations"])
}

/// Top-level directory names that hold CLI tools, build/automation scripts, and
/// benchmark harnesses run directly (by Node, a build runner, or a shell),
/// never `import`-ed as library modules. Matched only at the project root, so a
/// nested `src/scripts/` library helper does not qualify.
const TOP_LEVEL_SCRIPT_DIRS: &[&str] = &[
    "scripts",
    "bin",
    "tools",
    "examples",
    "example-apps",
    "benchmark",
    "benchmarks",
];

/// True when `path` lives under a top-level CLI/automation entry directory of
/// the project (e.g. `<root>/scripts/precompile.mjs`, `<root>/bin/cli.ts`).
/// These files are executed directly, never imported, so concerns that only
/// apply to importable library modules (dead exports, top-level side effects
/// blocking tree-shaking) do not apply to them. Anchoring on the project root
/// keeps a nested `src/scripts/util.ts` library module from being exempted.
pub fn is_top_level_script_dir_path(path: &Path, project_root: &Path) -> bool {
    let canon_path = canonicalize_cached(path);
    let canon_root = canonicalize_cached(project_root);
    let Ok(rel) = canon_path.strip_prefix(&canon_root) else {
        return false;
    };
    rel.components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
        .is_some_and(|first| TOP_LEVEL_SCRIPT_DIRS.contains(&first))
}

/// True for build/codegen scripts: files under a `scripts/` or `config/`
/// directory, or a root-level `build`/`bundle` script (e.g. `build.ts`,
/// `bundle.mjs`) sitting directly in `project_root`. Both run at dev/CI time
/// and are not part of the shipped bundle, so importing a devDependency from
/// them is correct.
pub fn is_build_script_path(path: &Path, project_root: &Path) -> bool {
    has_path_segment(path, &["scripts", "config"])
        || is_root_level_build_script(path, project_root)
}

/// True when `path` is a `build`/`bundle` script (stem `build` or `bundle`
/// with a JS/TS extension) sitting directly in `project_root`, rather than
/// under `src/` or any other subdirectory. Scoping to the project root keeps
/// a shipped `src/build.ts` from being mistaken for a tooling script.
fn is_root_level_build_script(path: &Path, project_root: &Path) -> bool {
    if path.parent() != Some(project_root) {
        return false;
    }
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs")
    ) && (stem == "build" || stem == "bundle")
}

/// True for demonstration code under `samples/`, `samples-dev/`, `examples/`,
/// `example/`, `example-apps/`, `demo/`, or `demos/`. Compiled and run at dev
/// time to show library usage; it never ships in the published package. (A
/// component library's `components/**/demo/` files are documentation examples
/// that legitimately import devDependencies — issue #1563.)
pub fn is_sample_dir_path(path: &Path) -> bool {
    has_path_segment(
        path,
        &[
            "samples",
            "samples-dev",
            "examples",
            "example",
            "example-apps",
            "demo",
            "demos",
        ],
    )
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

/// True when `path` lives under a `mock`, `mocks`, `__mocks__`, `fixtures`, or
/// `__fixtures__` directory: the conventional homes for test mock objects and
/// fixture data. Value-level mocks of a runtime config object mirror that
/// object's camelCase property names (`hasPluginDependencies`), and fixture
/// constants are scenario data, not application-wide compile-time invariants —
/// so naming conventions that require SCREAMING_SNAKE_CASE for top-level
/// constants do not apply (issue #1591). Segment match keeps an unrelated
/// `src/mockingbird/` or `src/fixturesHelper.ts` from matching.
pub fn is_mock_or_fixture_dir_path(path: &Path) -> bool {
    has_path_segment(path, &["mock", "mocks", "__mocks__", "fixtures", "__fixtures__"])
}

/// True when `path` lives under a `testing/` directory, the test-infrastructure
/// convention (popularised by bulletproof-react) for housing test utilities,
/// mock handlers, test setup files, and test-data generators co-located with
/// source. These files are loaded only by the test runner, never bundled into
/// production code, so they legitimately import test-only devDependencies.
/// Segment match keeps an unrelated `src/testingLibraryWrapper.ts` from matching.
pub fn is_test_infra_dir_path(path: &Path) -> bool {
    has_path_segment(path, &["testing"])
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

/// True when the file name carries a `.stories.` or `.story.` infix and a
/// JS/TS/MDX extension (e.g. `Button.stories.tsx`, `Header.story.js`,
/// `Intro.stories.mdx`), the Storybook story-file convention. Storybook
/// discovers these by the `stories: ["**/*.stories.@(ts|tsx|js|jsx|mdx)"]`
/// glob in `.storybook/main.*` and loads them at runtime — nothing `import`s
/// them, so the import-graph BFS cannot reach them, yet they are real entry
/// points, not dead code. The extension gate keeps an asset like
/// `Button.stories.css` (matched by a CSS tool, not Storybook's loader) out.
pub fn is_storybook_story(path: &Path) -> bool {
    if !matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mdx")
    ) {
        return false;
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.contains(".stories.") || name.contains(".story."))
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

/// True when the file name carries a `.actual.` or `.expected.` infix (e.g.
/// `theme.actual.js`, `color-imports.expected.ts`), the jscodeshift/babel
/// codemod snapshot convention. These files are input/output fixture snapshots
/// read as text by the codemod test harness — never imported, bundled, or
/// executed as modules — so their top-level code is intentional test data. The
/// infix is matched between dots (a leading dot is required), so an ordinary
/// `factual.js` does not match.
pub fn has_codemod_snapshot_infix(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains(".actual.") || lower.contains(".expected.")
        })
}

/// True when the file's basename stem ends with a PascalCase `Tests` or `Spec`
/// suffix (capital `T`/`S`), e.g. `apolloServerTests.ts`, `httpServerSpec.tsx`.
/// This is the test-suite-factory convention: files exporting `describe()`-block
/// factories (`defineIntegrationTestSuiteApolloServerTests`) consumed by real
/// `.test.ts` specs. They carry no `.test.`/`.spec.` infix, so the lowercase
/// substring scan misses them; the suffix is matched case-sensitively to avoid
/// flagging ordinary words like `manifests.ts` or `respec.ts`.
pub fn has_test_suite_factory_suffix(path: &Path) -> bool {
    const TEST_SUFFIXES: &[&str] = &["Tests", "Spec"];
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    if !matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs")
    ) {
        return false;
    }
    TEST_SUFFIXES.iter().any(|suffix| {
        let Some(prefix) = stem.strip_suffix(suffix) else {
            return false;
        };
        // Require a non-empty prefix ending in a lowercase letter or digit so the
        // suffix is a genuine camelCase word boundary (`serverTests`), not the
        // whole stem (`Tests.ts`) or another capitalized token (`HTTPSpec`).
        prefix
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
}

/// Strip a build-tool query/hash suffix from a relative or absolute path
/// specifier, returning the bare filesystem path. Vite (and other bundlers)
/// attach an import directive as a query string — `./file.txt?url`,
/// `./worker.js?worker`, `./image.png?raw&inline` — or a hash fragment that the
/// bundler consumes at build time and that is not part of the on-disk path. The
/// file resolver must stat `./file.txt`, not the literal `./file.txt?url`.
///
/// Only relative (`./`, `../`) and absolute (`/`) specifiers are stripped:
/// bare package specifiers never name an on-disk path here and are returned
/// unchanged, so a hypothetical `?` inside one is left intact. The suffix is cut
/// at the first `?` or `#`, whichever comes first, since a query precedes a hash
/// in URL grammar and the bundler treats everything after either as a directive.
#[must_use]
pub fn strip_specifier_query(spec: &str) -> &str {
    if !(spec.starts_with("./") || spec.starts_with("../") || spec.starts_with('/')) {
        return spec;
    }
    let cut = spec.find(['?', '#']).unwrap_or(spec.len());
    &spec[..cut]
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
///
/// Consults both the root-detected frameworks and the framework owning `path`
/// via its nearest `package.json`: in a monorepo the framework dependency may be
/// declared only in a nested sub-package (a Next.js playground whose `next` dep
/// is in `playgrounds/next/package.json`), invisible to root-anchored detection.
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

    for fw in project.frameworks_for_path(path) {
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

/// True when `path` lives under a framework entry_points.dirs directory.
/// Used by `dead-export` to suppress the dirs bail-out only when the user
/// has NOT configured additional entrypoints (backward-compat mode).
///
/// Consults both the root-detected frameworks and the framework owning `path`
/// via its nearest `package.json`, so a file-system-routed directory (Next.js
/// `/app/`, Remix `routes/`) is recognized even when the framework dependency
/// lives only in a nested sub-package, invisible to root-anchored detection.
pub fn is_in_framework_entry_dir(path: &Path, project: &ProjectCtx) -> bool {
    let path_str = path.to_string_lossy().replace('\\', "/");
    if project
        .framework_entry_dirs()
        .any(|dir| path_str.contains(dir))
    {
        return true;
    }
    project.frameworks_for_path(path).iter().any(|fw| {
        fw.entry_points
            .dirs
            .iter()
            .any(|dir| path_str.contains(dir.as_str()))
    })
}

/// True when `file_name` is a SvelteKit route file: a `+`-prefixed basename
/// from the framework's file-system routing set (`+page`, `+layout`,
/// `+server`, `+error` with `.svelte`/`.ts`/`.js` and an optional `.server`
/// segment). `+page`/`+layout` may carry the layout-break `@<group>` syntax
/// (`+page@.svelte`, `+layout@(group).ts`) that reparents the route's layout.
/// These are discovered by the router at build time, never imported.
pub fn is_sveltekit_route_file(file_name: &str) -> bool {
    let Some(rest) = file_name.strip_prefix('+') else {
        return false;
    };
    let mut parts: Vec<&str> = rest.split('.').collect();
    // SvelteKit's layout-break syntax appends `@<group>` to the `page`/`layout`
    // base segment (`+page@.js`, `+page@(admin).svelte`), reparenting which
    // layout the route inherits. The `@` and everything after it up to the
    // first `.` is the layout target, not part of the route kind — strip it so
    // the base matches the same `page`/`layout` patterns as the plain form.
    // Only `page`/`layout` carry this syntax, so an `@` after any other kind is
    // left intact and stays a non-match.
    if let Some(base) = parts.first_mut()
        && let Some((kind @ ("page" | "layout"), _)) = base.split_once('@')
    {
        *base = kind;
    }
    matches!(
        parts.as_slice(),
        ["page" | "layout" | "error", "svelte"]
            | ["page" | "layout", "js" | "ts"]
            | ["page" | "layout", "server", "js" | "ts"]
            | ["server", "js" | "ts"]
    )
}

/// True when `path` is a SvelteKit route-parameter matcher: a `.js`/`.ts` file
/// directly under a `params/` directory (`src/params/integer.ts`). Each such
/// file exports a `match` function the router calls to validate a `[x=name]`
/// route segment — consumed by file convention, never imported.
pub fn is_sveltekit_param_matcher_file(path: &Path) -> bool {
    let is_script = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("js" | "ts")
    );
    if !is_script {
        return false;
    }
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .is_some_and(|dir| dir == "params")
}

/// True when `path` is a Remix (React Router v7) route module: a file directly
/// or transitively under an `app/routes/` directory. Remix's file-system router
/// consumes the route conventions (`loader`, `action`, `meta`, `default`, …) by
/// exact name, never through a static import, so they have no importer but are
/// live. The `app/routes/` ancestor scopes the exemption to route modules,
/// keeping a same-named export in an ordinary module flaggable.
pub fn is_remix_route_file(path: &Path) -> bool {
    let mut components = path.components();
    while let Some(component) = components.next() {
        if component.as_os_str() == std::ffi::OsStr::new("app")
            && components
                .clone()
                .next()
                .is_some_and(|next| next.as_os_str() == std::ffi::OsStr::new("routes"))
        {
            return true;
        }
    }
    false
}

/// True when `path` is a React Router v7 app root module (`root.tsx`/`root.jsx`).
/// Its `Layout`, `meta`, `links`, and default exports are consumed by the
/// framework's render pipeline by exact name, never through a static import, so
/// they have no importer but are live. Scoping is provided by the caller's
/// framework-detection gate (React Router v7 framework mode), so basename match
/// is enough here.
pub fn is_react_router_root_module(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("root.tsx" | "root.jsx")
    )
}

/// True when `path` is a React Router v7 route-configuration entry
/// (`routes.ts`/`routes.js`). Its `default` export is consumed by
/// `@react-router/dev`, never through a static import. As with the root module,
/// the caller's framework-detection gate scopes the exemption.
pub fn is_react_router_routes_config(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("routes.ts" | "routes.js")
    )
}

/// True when `path` is a Docusaurus theme swizzle component — a file under a
/// `src/theme/` directory (consecutive `src` then `theme` segments, e.g.
/// `src/theme/MDXComponents/index.tsx`). Docusaurus's theme system discovers
/// these overrides by their path under `src/theme/` and resolves them through
/// its webpack theme aliases, never through a static import, so the component's
/// `default` export has no importer yet is live. Detection-gated by the caller.
pub fn is_docusaurus_theme_swizzle(path: &Path) -> bool {
    let segs: Vec<&str> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    segs.windows(2).any(|w| w == ["src", "theme"])
}

/// True when `path` is a Docusaurus plugin entry — an `index.{ts,js}` file
/// directly inside a `plugins/<name>/` directory (e.g.
/// `plugins/recent-blog-posts/index.ts`). Docusaurus loads local plugins by
/// the path string declared in `docusaurus.config`, calling the module's
/// `default` export, never through a static import, so it has no importer yet
/// is live. Detection-gated by the caller.
pub fn is_docusaurus_plugin_entry(path: &Path) -> bool {
    if !matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("index.ts" | "index.js")
    ) {
        return false;
    }
    let Some(grandparent) = path.parent().and_then(Path::parent) else {
        return false;
    };
    grandparent.file_name().and_then(|n| n.to_str()) == Some("plugins")
}

/// True when `path` is an Astro file-system-routed module: a file under a
/// `pages/` or `content/` directory. Astro's router consumes a route module's
/// reserved exports (`default`, the `GET`/`POST`/… HTTP method handlers,
/// `getStaticPaths`, `prerender`, `partial`) by exact name, never through a
/// static import, so they have no importer but are live. The `pages/`/`content/`
/// ancestor scopes the exemption to route modules, keeping a same-named export
/// in an ordinary module flaggable. Detection-gated by the caller.
pub fn is_astro_routed_page(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c.as_os_str().to_str(), Some("pages" | "content")))
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
    fn top_level_script_dir_is_root_anchored() {
        let root = PathBuf::from("/repo");
        for dir in ["scripts", "bin", "tools", "examples", "example-apps", "benchmark", "benchmarks"] {
            assert!(
                is_top_level_script_dir_path(&root.join(format!("{dir}/run.ts")), &root),
                "top-level {dir}/ is a CLI/automation entry dir"
            );
        }
        // Issue #1657: an inline build script under top-level `scripts/`.
        assert!(is_top_level_script_dir_path(&root.join("scripts/precompile.mjs"), &root));
        // Anchored at the root only: a nested `src/scripts/` library helper is
        // a real importable module and must NOT be exempted.
        assert!(!is_top_level_script_dir_path(&root.join("src/scripts/util.ts"), &root));
        assert!(!is_top_level_script_dir_path(&root.join("src/app/login.ts"), &root));
    }

    #[test]
    fn build_script_and_sample_dir_segments() {
        let root = PathBuf::from("/repo");
        assert!(is_build_script_path(&root.join("scripts/gen.ts"), &root));
        assert!(is_build_script_path(&root.join("config/helpers.ts"), &root));
        assert!(!is_build_script_path(&root.join("src/appconfig/index.ts"), &root));
        // Root-level build/bundle scripts are tooling, exempt at the root only.
        assert!(is_build_script_path(&root.join("build.ts"), &root));
        assert!(is_build_script_path(&root.join("bundle.mjs"), &root));
        assert!(!is_build_script_path(&root.join("src/build.ts"), &root));
        assert!(!is_build_script_path(&root.join("app.ts"), &root));
        assert!(is_sample_dir_path(&PathBuf::from("samples-dev/x.ts")));
        assert!(is_sample_dir_path(&PathBuf::from("examples/app.ts")));
        // Issue #1563: component-library demo/example directories.
        assert!(is_sample_dir_path(&PathBuf::from("components/tabs/demo/style-class.tsx")));
        assert!(is_sample_dir_path(&PathBuf::from("packages/foo/demos/index.tsx")));
        assert!(is_sample_dir_path(&PathBuf::from("example/app.ts")));
        assert!(!is_sample_dir_path(&PathBuf::from("src/mysamples/index.ts")));
        // Segment (not substring) match — `demonstration` must not match `demo`.
        assert!(!is_sample_dir_path(&PathBuf::from("src/demonstration/index.ts")));
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
    fn mock_or_fixture_dir_path_segments() {
        // Issue #1591: mock config-flag and fixture constants under a
        // mock/fixture directory.
        assert!(is_mock_or_fixture_dir_path(&PathBuf::from("test/mocks/nuxt-config.ts")));
        assert!(is_mock_or_fixture_dir_path(&PathBuf::from("src/mock/config.ts")));
        assert!(is_mock_or_fixture_dir_path(&PathBuf::from("__mocks__/fs.js")));
        assert!(is_mock_or_fixture_dir_path(&PathBuf::from("test/fixtures/data.ts")));
        assert!(is_mock_or_fixture_dir_path(&PathBuf::from("__fixtures__/sample.ts")));
        // Segment (not substring) match.
        assert!(!is_mock_or_fixture_dir_path(&PathBuf::from("src/mockingbird/index.ts")));
        assert!(!is_mock_or_fixture_dir_path(&PathBuf::from("src/fixturesHelper.ts")));
        assert!(!is_mock_or_fixture_dir_path(&PathBuf::from("src/app/login.ts")));
    }

    #[test]
    fn test_infra_dir_path_segments() {
        // Issue #1756: the `testing/` test-infrastructure convention.
        assert!(is_test_infra_dir_path(&PathBuf::from(
            "apps/react-vite/src/testing/mocks/utils.ts"
        )));
        assert!(is_test_infra_dir_path(&PathBuf::from(
            "apps/react-vite/src/testing/setup-tests.ts"
        )));
        // Segment (not substring) match.
        assert!(!is_test_infra_dir_path(&PathBuf::from("src/testingLibraryWrapper.ts")));
        assert!(!is_test_infra_dir_path(&PathBuf::from("src/app/login.ts")));
    }

    #[test]
    fn fuzz_targets_path_match() {
        assert!(is_fuzz_targets_path(&PathBuf::from("fuzz/fuzz_targets/parse.rs")));
        assert!(!is_fuzz_targets_path(&PathBuf::from("src/lib.rs")));
    }

    #[test]
    fn angular_schematic_or_migration_entry_segments() {
        // Issue #1597: ngrx/platform schematic and ng-update migration entry points.
        assert!(is_angular_schematic_or_migration_entry(&PathBuf::from(
            "modules/effects/schematics/ng-add/index.ts"
        )));
        assert!(is_angular_schematic_or_migration_entry(&PathBuf::from(
            "modules/router-store/migrations/14_0_0/index.ts"
        )));
        // Segment (not substring) match.
        assert!(!is_angular_schematic_or_migration_entry(&PathBuf::from(
            "src/migrationsHelper.ts"
        )));
        assert!(!is_angular_schematic_or_migration_entry(&PathBuf::from(
            "src/components/index.ts"
        )));
    }

    #[test]
    fn test_suite_factory_suffix_match() {
        // Issue #1661: PascalCase `Tests`/`Spec` test-suite-factory convention.
        assert!(has_test_suite_factory_suffix(&PathBuf::from(
            "packages/integration-testsuite/src/apolloServerTests.ts"
        )));
        assert!(has_test_suite_factory_suffix(&PathBuf::from("src/httpServerTests.js")));
        assert!(has_test_suite_factory_suffix(&PathBuf::from("src/queryTests.tsx")));
        assert!(has_test_suite_factory_suffix(&PathBuf::from("src/parserSpec.ts")));
        // Negative space: ordinary words that merely end in lowercase `tests`/`spec`
        // (no capital boundary) are production files, not test factories.
        assert!(!has_test_suite_factory_suffix(&PathBuf::from("src/manifests.ts")));
        assert!(!has_test_suite_factory_suffix(&PathBuf::from("src/respec.ts")));
        // The suffix must be a camelCase word boundary, not the whole stem.
        assert!(!has_test_suite_factory_suffix(&PathBuf::from("src/Tests.ts")));
        assert!(!has_test_suite_factory_suffix(&PathBuf::from("src/Spec.ts")));
        // Only JS/TS source extensions qualify.
        assert!(!has_test_suite_factory_suffix(&PathBuf::from("src/apolloServerTests.md")));
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
    fn storybook_story_infix_and_extension() {
        // Issue #1359: Storybook story files discovered via the glob loader.
        assert!(is_storybook_story(&PathBuf::from("components/Button.stories.tsx")));
        assert!(is_storybook_story(&PathBuf::from("src/Header.stories.ts")));
        assert!(is_storybook_story(&PathBuf::from("src/Card.stories.jsx")));
        assert!(is_storybook_story(&PathBuf::from("src/Card.stories.js")));
        assert!(is_storybook_story(&PathBuf::from("docs/Intro.stories.mdx")));
        // Older singular `.story.` convention.
        assert!(is_storybook_story(&PathBuf::from("src/Toggle.story.tsx")));
        // The extension gate keeps non-loaded sibling assets out.
        assert!(!is_storybook_story(&PathBuf::from("src/Button.stories.css")));
        assert!(!is_storybook_story(&PathBuf::from("src/Button.stories.json")));
        // An ordinary component file is not a story.
        assert!(!is_storybook_story(&PathBuf::from("src/Button.tsx")));
        // `stories`/`story` must be a `.`-delimited infix, not a stem substring.
        assert!(!is_storybook_story(&PathBuf::from("src/stories.ts")));
        assert!(!is_storybook_story(&PathBuf::from("src/storyboard.ts")));
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
    fn strip_specifier_query_drops_vite_directives() {
        // Issue #1582: Vite query-string asset imports — the `?…` directive is a
        // build-time transform, not part of the on-disk path.
        assert_eq!(strip_specifier_query("./file.txt?url"), "./file.txt");
        assert_eq!(strip_specifier_query("./image.png?raw"), "./image.png");
        assert_eq!(strip_specifier_query("./worker.js?worker"), "./worker.js");
        assert_eq!(strip_specifier_query("./data.csv?url&inline"), "./data.csv");
        assert_eq!(strip_specifier_query("../[url].txt?url"), "../[url].txt");
        assert_eq!(strip_specifier_query("/abs/styles.css?url"), "/abs/styles.css");
        // A hash fragment is cut too; a query before a hash wins (URL grammar).
        assert_eq!(strip_specifier_query("./mod.js#frag"), "./mod.js");
        assert_eq!(strip_specifier_query("./mod.js?url#frag"), "./mod.js");
        // No suffix: returned unchanged.
        assert_eq!(strip_specifier_query("./file.txt"), "./file.txt");
        // Bare package specifiers are never on-disk paths here — left intact.
        assert_eq!(strip_specifier_query("react?weird"), "react?weird");
        assert_eq!(strip_specifier_query("@scope/pkg/sub"), "@scope/pkg/sub");
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

    #[test]
    fn sveltekit_route_file_matches_plain_and_layout_break_forms() {
        // Plain file-router forms.
        for name in [
            "+page.svelte",
            "+layout.svelte",
            "+error.svelte",
            "+page.ts",
            "+layout.js",
            "+page.server.ts",
            "+server.js",
        ] {
            assert!(is_sveltekit_route_file(name), "{name} is a SvelteKit route file");
        }
        // Issue #1608: layout-break `@<group>` syntax on `+page`/`+layout`.
        for name in [
            "+page@.js",
            "+page@group.js",
            "+page@(admin).svelte",
            "+page@.server.ts",
            "+layout@.svelte",
            "+layout@reset.ts",
        ] {
            assert!(is_sveltekit_route_file(name), "{name} is a SvelteKit layout-break route file");
        }
        // Negative space: a genuinely misnamed file must still be flagged.
        assert!(!is_sveltekit_route_file("PageComponent.svelte"));
        assert!(!is_sveltekit_route_file("+widget.svelte"));
        assert!(!is_sveltekit_route_file("page.svelte"));
        // `@` is only the layout-break marker on `page`/`layout`; other kinds
        // carrying it are not framework route files.
        assert!(!is_sveltekit_route_file("+server@.js"));
        assert!(!is_sveltekit_route_file("+error@.svelte"));
    }

    #[test]
    fn remix_route_file_matches_app_routes_paths() {
        for rel in [
            "app/routes/index.tsx",
            "app/routes/api.v1.projects.$projectRef.ts",
            "app/routes/_auth.login.tsx",
            "apps/webapp/app/routes/vercel.install.tsx",
        ] {
            assert!(
                is_remix_route_file(Path::new(rel)),
                "{rel} is a Remix route module"
            );
        }
        // Negative space: `routes/` not under `app/`, an `app/` dir without a
        // `routes/` child, and an ordinary module must not match.
        assert!(!is_remix_route_file(Path::new("src/routes/index.tsx")));
        assert!(!is_remix_route_file(Path::new("app/lib/data.ts")));
        assert!(!is_remix_route_file(Path::new("app/root.tsx")));
        assert!(!is_remix_route_file(Path::new("routes/index.tsx")));
    }

    #[test]
    fn react_router_root_module_matches_root_tsx_and_jsx() {
        assert!(is_react_router_root_module(Path::new("app/root.tsx")));
        assert!(is_react_router_root_module(Path::new("docs/app/root.jsx")));
        // Negative space: TanStack's `__root.tsx`, a route file, and an ordinary
        // module must not match.
        assert!(!is_react_router_root_module(Path::new("src/routes/__root.tsx")));
        assert!(!is_react_router_root_module(Path::new("app/routes/index.tsx")));
        assert!(!is_react_router_root_module(Path::new("app/root.css")));
    }

    #[test]
    fn react_router_routes_config_matches_routes_ts_and_js() {
        assert!(is_react_router_routes_config(Path::new("app/routes.ts")));
        assert!(is_react_router_routes_config(Path::new("docs/app/routes.js")));
        // Negative space: the `routes/` directory and an ordinary module name.
        assert!(!is_react_router_routes_config(Path::new("app/routes/index.tsx")));
        assert!(!is_react_router_routes_config(Path::new("app/route.ts")));
    }
}
