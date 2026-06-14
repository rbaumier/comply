//! dead-export detection — walk every export in the current file and verify
//! it has at least one linked importer in the index.
//!
//! Skips:
//!   - Test files (`*.test.*`, `*.spec.*`, `tests/`, `__tests__/`) — these
//!     may legitimately export fixtures used only internally.
//!   - Storybook Component Story Format files (`*.stories.*` and files under a
//!     Storybook directory, via `path_segments.in_storybook`) — each named
//!     export is a Story the Storybook runtime discovers by glob and the
//!     `default` export is the meta object. They are consumed at runtime, never
//!     through a static import, so the import graph shows no importer.
//!   - Entry points (`main.*`, `index.*` at the project root) — they are the
//!     consumer, not the consumed, and aren't imported by convention.
//!   - CLI-tool packages (the nearest `package.json` declares a `bin`) — the
//!     package's `src/**` implements one or more published binaries. Sibling
//!     packages consume it by invoking the binary, not by ES-importing its
//!     modules, and the tool's command framework wires up internal modules
//!     dynamically, so their exports have no static importer.
//!   - Star re-exports (`export * from './m'`) — the re-export doesn't carry
//!     a specific name to link against; it's a barrel, not a dead symbol.
//!   - Reusable UI library directories (`components/ui/`, `lib/ui/`) — these
//!     hold drop-in components (shadcn convention) that are installed for
//!     future use; flagging them every time a developer adds one before its
//!     first import is pure noise.
//!   - Generated files (containing a `// @generated` or `/* @generated */`
//!     header in the first ~40 lines) — code generators emit a fixed export
//!     surface that callers may pick from gradually.
//!   - Fixture / test-data directories (`__fixtures__/`, `fixtures/`,
//!     `test-data/`, `testdata/`, …) — these hold factories and fixtures
//!     consumed only from test files, often through tooling-generated path
//!     aliases (e.g. SvelteKit's `@test-data`) that the index can't resolve,
//!     so their exports look unimported even though tests use them.
//!   - Co-occurrence conventions (`CO_OCCURRENCE_EXEMPTIONS`) — a fixed set of
//!     named exports consumed by directory/filename convention at runtime,
//!     never through a static import. Each convention fires only when its two
//!     gate names co-occur in the module's export set. They are deliberately
//!     project-agnostic: the export-shape co-occurrence identifies the
//!     convention without needing a detected framework. Two conventions today —
//!     yargs command modules (gated on `command` AND `handler`, alongside
//!     `builder`/`describe`/`description`/`aliases`/`deprecated`), loaded
//!     dynamically by yargs via `commandDir()` / `.command(require(...))`; and
//!     database migration modules (gated on `up` AND `down`, the canonical
//!     signature shared by Kysely, TypeORM, Prisma, Sequelize, Knex,
//!     node-pg-migrate, …), discovered and run by the ORM's migration runner.
//!   - Nextra meta files (`_meta.{tsx,ts,js,jsx}`) — Nextra's file-system
//!     router consumes the per-directory `default` route-metadata export by
//!     filename convention at build time, so it never has a static importer.
//!     The whole file is treated as a framework entry point. The leading
//!     underscore is required: an ordinary `meta.ts` stays subject to the rule.
//!   - Docusaurus theme swizzle and plugin `default` exports — when Docusaurus
//!     is detected for the file (root or nearest package.json), a component
//!     under a `src/theme/` directory and a plugin `index.{ts,js}` directly
//!     inside a `plugins/<name>/` directory have their `default` export
//!     consumed by Docusaurus's theme system / config-driven plugin loader by
//!     path convention, never through a static import. Only `default` is magic
//!     and only when Docusaurus is detected, so named exports and non-Docusaurus
//!     projects stay subject to the rule. (`knip.ts`/`knip.js` are handled by
//!     the shared `is_config_file` predicate alongside `knip.config.*`.)
//!   - Serverless `handler` exports under a `functions/` directory — a file in
//!     the per-function layout AWS Lambda / SST / Cloudflare Workers / Vercel
//!     Edge use exports `handler`, which the cloud runtime invokes through the
//!     deploy config's `handler: "functions/my-fn/index.handler"` string, never
//!     through a static TS import. The exemption is gated on BOTH the
//!     `functions/` directory and the `handler` name, so a lone `handler`
//!     export elsewhere stays subject to the rule.
//!   - Node.js ESM customization-hook exports (`resolve`/`load`/`globalPreload`)
//!     in an `.mjs`/`.mts` module declared with the canonical chained-hook
//!     signature — Node loads the module through the `--loader`/`--import` (or
//!     `register(...)`) machinery and invokes those exports by name, never
//!     through a static import. Gated on BOTH the ESM file convention AND the
//!     hook shape (`resolve`/`load`'s last parameter is the `nextResolve`/
//!     `nextLoad` continuation; `globalPreload` rides on a shape-valid sibling),
//!     so an ordinary `export const resolve = …` in a `.ts`/`.js` file, or one
//!     without the chained-hook signature, stays subject to the rule.
//!   - Framework file-system-routing entry points (`is_framework_route_export`) —
//!     a file matching a well-known routing convention exposes reserved exports
//!     that the framework's router consumes by name, never through a static
//!     import. Recognized conventions: Next.js Pages Router (`pages/**`, incl.
//!     `pages/api/**`), Next.js App Router special files (`page`/`layout`/`route`/
//!     `loading`/`error`/`not-found`/`template`/`default`/`global-error` under an
//!     `app/` directory), Remix / React Router v7 route modules (`routes/**`) and
//!     the app root (`root.{tsx,jsx}`), and SvelteKit route files (`+page`/
//!     `+layout`/`+server` and their `.server` variants). This check is
//!     path-convention-based and does NOT require the framework dependency to be
//!     detected, so it covers monorepo
//!     route files whose framework dep is invisible to nearest-manifest
//!     detection. Only the convention's reserved export names are exempt, so a
//!     genuinely-dead helper in a route file still fires.
//!
//! False-positive guards:
//!   - If any file imports the current module via a namespace import
//!     (`import * as ns from './m'`), `symbol_usages` is intentionally not
//!     populated for individual names. In that case every export on the
//!     module is treated as live — we can't tell from the index alone which
//!     specific names `ns.*` accesses touch.
//!   - `export default` is matched against the `"default"` usage key.
//!   - Exported types/interfaces that parameterize the signature of another
//!     exported function in the same file are kept — callers consume them
//!     structurally (by passing an object literal to that function) without
//!     ever importing the type name.
//!   - Exports referenced anywhere else in the same file (schema chains like
//!     `BaseSchema.extend(...)`, `z.infer<typeof BaseSchema>`, composition
//!     into another exported value) are kept. The base name is consumed
//!     in-file; its derived form is what callers import.
//!   - Files referenced through a Docusaurus `@site/` alias import are kept.
//!     Docusaurus maps `@site/` to the site root via webpack, so
//!     `import X from "@site/src/components/foo"` never resolves in the index
//!     and the imported component looks dead. When an unresolved `@site/`
//!     specifier's path suffix matches a file, every export of it is live.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::is_custom_element_decorator_name;
use crate::parsing::ts_language_for;
use crate::project::import_index::ExportKind;
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::path_utils::{
    is_config_file, is_framework_specific_entry_point, is_in_framework_entry_dir,
    is_sample_dir_path,
};
use crate::rules::walker::walk_tree;
use std::collections::HashSet;
use std::path::Path;

const RULE_ID: &str = "dead-export";

/// Path segments that mark a directory as a reusable UI component library.
/// Matched against the canonicalised path with forward-slash separators.
const UI_LIBRARY_DIRS: &[&str] = &["/components/ui/", "/lib/ui/", "/src/components/ui/"];

fn is_in_ui_library(path: &Path) -> bool {
    let normalised = path.to_string_lossy().replace('\\', "/");
    UI_LIBRARY_DIRS.iter().any(|seg| normalised.contains(seg))
}

const FIXTURE_DIRS: &[&str] = &[
    "__testfixtures__",
    "__fixtures__",
    "fixtures",
    "test-fixtures",
    "test-data",
    "testdata",
];

fn is_in_fixture_dir(path: &Path) -> bool {
    let normalised = path.to_string_lossy().replace('\\', "/");
    FIXTURE_DIRS.iter().any(|seg| normalised.contains(seg))
}

/// True when `path`'s basename is a Nextra meta file (`_meta.tsx`, `_meta.ts`,
/// `_meta.js`, `_meta.jsx`). Nextra's file-system router consumes the
/// per-directory `default` route-metadata export by this filename convention at
/// build time, so the file never appears as an importer in the index. The
/// leading underscore is required so an ordinary `meta.ts` is not exempted.
fn is_nextra_meta_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("_meta.tsx" | "_meta.ts" | "_meta.js" | "_meta.jsx")
    )
}

/// Export name a serverless platform invokes by configuration string rather
/// than a static import (`handler: "functions/my-fn/index.handler"`).
const SERVERLESS_HANDLER_EXPORT: &str = "handler";

/// True when `path` lives under a `functions/` directory — the per-function
/// layout serverless platforms (AWS Lambda, SST, Cloudflare Workers, Vercel
/// Edge) use, where each function gets its own directory and the deploy config
/// references the entry file's `handler` export by path. Segment (not substring)
/// match keeps `src/functionsRegistry/` from qualifying.
fn is_in_serverless_functions_dir(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(c, std::path::Component::Normal(s) if s.to_str() == Some("functions"))
    })
}

/// True when `path` is an explicit-ESM `.mjs`/`.mts` module — the file
/// convention a Node.js customization-hooks module follows (the hooks API is
/// ESM-only). Node loads such a module through the `--loader`/`--import` (or
/// `register(...)`) machinery, so its `resolve`/`load`/`globalPreload` exports
/// have no static importer. Requiring this extension keeps an ordinary
/// `resolve`/`load` export in a `.ts`/`.js` file subject to the rule; it is the
/// first half of the signal, paired with the stronger chained-hook shape gate.
fn is_node_loader_hook_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("mjs" | "mts")
    )
}

/// TS/JS source extensions a file-system-routed framework module can carry.
const ROUTE_MODULE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"];

/// True when `path`'s extension is a TS/JS source extension a framework router
/// can load as a route module.
fn has_route_module_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ROUTE_MODULE_EXTENSIONS.contains(&ext))
}

/// True when `path` has `segment` as one of its directory components (exact
/// segment, not substring), so `pages/` matches `app/pages/index.tsx` but not
/// `src/subpages/x.ts`.
fn has_path_segment(path: &Path, segment: &str) -> bool {
    path.components()
        .any(|c| matches!(c, std::path::Component::Normal(s) if s.to_str() == Some(segment)))
}

/// Next.js App Router special-file stems: each is consumed by the App Router by
/// filename convention under an `app/` directory.
const NEXT_APP_ROUTER_STEMS: &[&str] = &[
    "page",
    "layout",
    "route",
    "loading",
    "error",
    "not-found",
    "template",
    "default",
    "global-error",
];

/// Reserved exports a Next.js App Router special file (`page`/`layout`/`route`/…)
/// exposes for the framework to consume by name: the default component/handler,
/// the segment-config directives, the metadata API, and the HTTP-method handlers
/// of a `route` file. Never reached through a static import.
const NEXT_APP_ROUTER_EXPORTS: &[&str] = &[
    "default",
    "generateStaticParams",
    "generateMetadata",
    "metadata",
    "generateViewport",
    "viewport",
    "revalidate",
    "dynamic",
    "dynamicParams",
    "fetchCache",
    "runtime",
    "preferredRegion",
    "maxDuration",
    "config",
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "HEAD",
    "OPTIONS",
];

/// Reserved exports a Next.js Pages Router module (`pages/**`, incl. `pages/api/**`)
/// exposes for the framework: the default page component / API handler and the
/// data-fetching hooks read by name at build/request time.
const NEXT_PAGES_ROUTER_EXPORTS: &[&str] = &[
    "default",
    "getServerSideProps",
    "getStaticProps",
    "getStaticPaths",
    "config",
];

/// Reserved exports a Remix / React Router v7 route module (`routes/**`) or the
/// app root module (`root.{tsx,jsx}`) exposes for the framework's render
/// pipeline, consumed by exact name and never imported.
const REMIX_ROUTE_EXPORTS: &[&str] = &[
    "default",
    "loader",
    "action",
    "meta",
    "links",
    "headers",
    "ErrorBoundary",
    "CatchBoundary",
    "handle",
    "shouldRevalidate",
    "clientLoader",
    "clientAction",
    "HydrateFallback",
    "Layout",
];

/// Reserved exports a SvelteKit route module (`+page`/`+layout`/`+server`,
/// incl. the `.server` variants) exposes for its file-system router, plus
/// `default` for the `.svelte` component module.
const SVELTEKIT_ROUTE_EXPORTS: &[&str] = &[
    "default",
    "load",
    "ssr",
    "csr",
    "prerender",
    "trailingSlash",
    "config",
    "actions",
    "entries",
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "OPTIONS",
    "HEAD",
    "fallback",
];

/// True when `export_name` is consumed by a framework file-system router for a
/// file matching a well-known routing convention, so it has no static importer
/// yet is live. Detection is purely path-convention-based — independent of
/// whether the framework dependency is visible in the nearest `package.json`,
/// which is the gap that surfaced the false positive (a monorepo route file
/// whose `next`/`@remix-run` dep is declared out of reach of nearest-manifest
/// detection). Matching is anchored on exact path SEGMENTS and filenames, so an
/// ordinary module never qualifies, and only the convention's reserved export
/// names are exempt, so a genuinely-dead helper in a route file still fires.
fn is_framework_route_export(path: &Path, export_name: &str) -> bool {
    if !has_route_module_extension(path) {
        return false;
    }
    let basename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    // Next.js App Router special files under an `app/` directory.
    if has_path_segment(path, "app")
        && NEXT_APP_ROUTER_STEMS.contains(&stem)
        && NEXT_APP_ROUTER_EXPORTS.contains(&export_name)
    {
        return true;
    }

    // Next.js Pages Router (`pages/**`, including `pages/api/**`).
    if has_path_segment(path, "pages") && NEXT_PAGES_ROUTER_EXPORTS.contains(&export_name) {
        return true;
    }

    // Remix / React Router v7: route modules under a `routes/` directory, and the
    // app root module `root.{tsx,jsx}`.
    if (has_path_segment(path, "routes")
        || crate::rules::path_utils::is_react_router_root_module(path))
        && REMIX_ROUTE_EXPORTS.contains(&export_name)
    {
        return true;
    }

    // SvelteKit route files (`+page`, `+layout`, `+server`, and `.server` variants).
    if crate::rules::path_utils::is_sveltekit_route_file(basename)
        && SVELTEKIT_ROUTE_EXPORTS.contains(&export_name)
    {
        return true;
    }

    false
}

/// A framework convention whose named exports are discovered dynamically by a
/// runtime/build tool and therefore never have a static importer.
///
/// `gate` is the pair of names that must BOTH be present in the file's export
/// set for the convention to apply — the co-occurrence requirement that keeps
/// an ordinary module merely exporting one signature name from being
/// blanket-exempted. `exempt` is the full set of names treated as live once the
/// gate matches.
struct CoOccurrenceExemption {
    gate: [&'static str; 2],
    exempt: &'static [&'static str],
}

/// Conventions where a fixed set of named exports is consumed by directory /
/// filename convention at runtime or build time, never through a static import.
/// Each entry is gated on the co-occurrence of two signature names so a module
/// merely exporting one of them is not blanket-exempted.
///
/// These are deliberately project-agnostic (not gated on a detected framework):
/// the export-shape co-occurrence is specific enough to identify the convention
/// on its own, and the conventions predate dependency detection here.
const CO_OCCURRENCE_EXEMPTIONS: &[CoOccurrenceExemption] = &[
    // yargs command modules — yargs discovers these dynamically via
    // `commandDir()` / `.command(require(...))`.
    CoOccurrenceExemption {
        gate: ["command", "handler"],
        exempt: &[
            "command",
            "handler",
            "builder",
            "describe",
            "description",
            "aliases",
            "deprecated",
        ],
    },
    // Database migration modules — ORM migration runners (Kysely, TypeORM,
    // Prisma, Sequelize, Knex, node-pg-migrate, …) discover `up`/`down` by
    // directory convention and call them at runtime.
    CoOccurrenceExemption {
        gate: ["up", "down"],
        exempt: &["up", "down"],
    },
];

/// True when `export_name` is exempt under any co-occurrence convention whose
/// gate is satisfied by the module's `export_names`. A convention contributes
/// its `exempt` names only when BOTH of its gate names co-occur, preserving the
/// per-convention co-occurrence requirement.
fn is_co_occurrence_exempt(export_name: &str, export_names: &HashSet<&str>) -> bool {
    CO_OCCURRENCE_EXEMPTIONS.iter().any(|conv| {
        conv.gate.iter().all(|g| export_names.contains(g)) && conv.exempt.contains(&export_name)
    })
}

/// True when the project contains a Docusaurus `@site/`-aliased import whose
/// path plausibly references `exporting_file`. Docusaurus maps the `@site/`
/// alias to the site root via its webpack config, so `import X from
/// "@site/src/components/foo"` never resolves in the import index (it is not a
/// `compilerOptions.paths` entry) and the imported component looks dead.
///
/// The alias suffix (the part after `@site/`) is matched against the exporting
/// file's path: the file is considered live when its path, with any TS/JS
/// extension stripped, ends with the suffix — directly or through an `index`
/// segment (`@site/src/components/foo` ↔ `…/src/components/foo/index.tsx`).
/// Anchoring on the `@site/` prefix and a path-suffix match keeps the
/// exemption tight: a genuinely dead export with no such importer still fires.
fn has_unresolved_site_alias_importer(
    index: &crate::project::import_index::ImportIndex,
    exporting_file: &Path,
) -> bool {
    const SITE_ALIAS: &str = "@site/";

    let file_norm = exporting_file.to_string_lossy().replace('\\', "/");
    let file_stem = strip_ts_extension(&file_norm);

    index.iter_imports().any(|imp| {
        if imp.source_path.is_some() {
            return false;
        }
        let Some(suffix) = imp.specifier.strip_prefix(SITE_ALIAS) else {
            return false;
        };
        let suffix = suffix.trim_end_matches('/');
        if suffix.is_empty() {
            return false;
        }
        file_stem == suffix
            || file_stem.ends_with(&format!("/{suffix}"))
            || file_stem == format!("{suffix}/index")
            || file_stem.ends_with(&format!("/{suffix}/index"))
    })
}

/// Names exported by `exporting_file` that an ng-packagr public-API entry barrel
/// re-exports — i.e. the file's contribution to an Angular library's published
/// surface. ng-packagr libraries publish through the build output's
/// `package.json`, so the entry barrel's `export { X } from './m'` is the only
/// consumer of `m`'s `X` and no source file imports it; such a symbol is live.
///
/// Walks each indexed file that is an ng-package entry file and, for every
/// re-export whose origin resolves to `exporting_file`, records the origin name
/// (the `local` side of `export { local as exported } from …`, else the
/// exported name). Returns the empty set when the project ships no ng-packagr
/// entry, so a non-Angular project pays only the entry-file probe.
fn collect_ng_package_reexported_names(
    index: &crate::project::import_index::ImportIndex,
    project: &crate::project::ProjectCtx,
    exporting_file: &Path,
) -> HashSet<String> {
    let mut out = HashSet::new();
    for barrel in index.indexed_paths() {
        if !project.is_ng_package_entry_file(barrel) {
            continue;
        }
        for exp in index.get_exports(barrel) {
            if !matches!(exp.kind, ExportKind::ReExport) {
                continue;
            }
            if index.reexport_target(barrel, &exp.name) != Some(exporting_file) {
                continue;
            }
            let origin_name = exp.local_name.clone().unwrap_or_else(|| exp.name.clone());
            out.insert(origin_name);
        }
    }
    out
}

/// Strip a single trailing TS/JS extension from a forward-slashed path.
fn strip_ts_extension(path: &str) -> &str {
    for ext in [".tsx", ".ts", ".jsx", ".js", ".mts", ".cts", ".mjs", ".cjs"] {
        if let Some(stem) = path.strip_suffix(ext) {
            return stem;
        }
    }
    path
}

/// True if the source carries a `@generated` marker in its leading comments.
/// Only scans the first 2KB to keep the cost bounded; generators always emit
/// the marker at the top of the file.
fn is_generated(source: &str) -> bool {
    let mut end = source.len().min(2048);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    source[..end].contains("@generated")
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir {
            return Vec::new();
        }
        if ctx.file.path_segments.in_storybook {
            return Vec::new();
        }
        if is_config_file(ctx.path) {
            return Vec::new();
        }
        if is_entry_point(ctx.path, ctx.project.project_root.as_deref()) {
            return Vec::new();
        }
        if is_in_ui_library(ctx.path) {
            return Vec::new();
        }
        if is_generated(ctx.source) {
            return Vec::new();
        }
        if is_in_fixture_dir(ctx.path) {
            return Vec::new();
        }
        // Files under a demonstration directory (`examples/`, `example/`,
        // `example-apps/`, `demo/`, `demos/`, `samples/`, …) are standalone
        // runnable demo apps: each has its own non-library `package.json` and a
        // nested entry file (e.g. `examples/svelte/.../src/main.ts` exporting
        // `export default app`) that nothing imports. The root-only entry-point
        // check misses these nested mains, so reuse the same sample-dir
        // classifier `unused-file` uses to keep the two rules in agreement.
        if is_sample_dir_path(ctx.path) {
            return Vec::new();
        }
        if is_nextra_meta_file(ctx.path) {
            return Vec::new();
        }
        if ctx.project.nearest_package_json(ctx.path).is_some_and(|pkg| {
            pkg.is_library
                || pkg.has_bin
                || is_script_entry_point(ctx.path, ctx.project.project_root.as_deref(), &pkg.script_entry_files)
        }) {
            return Vec::new();
        }

        let index = ctx.project.import_index();
        // `dead-export` is structurally cross-project — it needs to see
        // at least one OTHER file to count potential consumers. When
        // comply is invoked on a single file (pre-commit hook over a
        // staged-only diff, ad-hoc `comply src/shared/foo.ts`), the
        // index holds only the checked file and every export looks
        // dead. Skip in that mode; users have a workaround already in
        // place but the rule's premise can't be honoured.
        if index.total_files() < 2 {
            return Vec::new();
        }
        let canon = index.canonical(ctx.path);

        // User-declared entry files (server mains, CLI entries, workers) — never flagged.
        if ctx.project.entrypoints_contains(&canon) {
            return Vec::new();
        }
        // Framework entry FILE/SUFFIX/ROOT_FILE match — always bail out, except
        // React Router v7 convention modules (`root.tsx`, `routes.ts`). Those
        // are entry points for file-level rules (unused-file, side-effects) but
        // here only their reserved exports are framework-consumed; an ordinary
        // dead export in the same file is still genuinely dead. The per-export
        // magic-exports check below exempts just the reserved names.
        let is_rr_convention_module =
            crate::rules::path_utils::is_react_router_root_module(&canon)
                || crate::rules::path_utils::is_react_router_routes_config(&canon);
        if !is_rr_convention_module && is_framework_specific_entry_point(&canon, ctx.project) {
            return Vec::new();
        }
        // ng-packagr public-API entry file (`lib.entryFile` of an
        // `ng-package.json`) — the package entry point for an Angular library;
        // never imported by another source file, so never flagged.
        if ctx.project.is_ng_package_entry_file(&canon) {
            return Vec::new();
        }
        // Framework entry DIR match — bail out only when no user entrypoints are
        // configured (backward-compat). When entrypoints are set the user wants
        // backend-dir files checked; only the specific file/suffix matches above protect them.
        if ctx.project.entrypoint_globs.is_empty() && is_in_framework_entry_dir(&canon, ctx.project) {
            return Vec::new();
        }
        let exports = index.get_exports(&canon);
        if exports.is_empty() {
            return Vec::new();
        }

        // If any importer uses namespace-import form, treat every export as
        // live — the index doesn't track which properties of `ns.*` are read.
        if index.is_namespace_imported(&canon) {
            return Vec::new();
        }

        // A file under a template-literal dynamic-import directory
        // (`import(\`./locales/${lang}\`)`) is loaded by a computed path: every
        // export is reachable at runtime, but no static import names it.
        if index.is_under_dynamic_import_dir(&canon) {
            return Vec::new();
        }

        let magic = ctx.project.magic_exports_for_path(&canon);

        // Co-occurrence-driven exemptions (yargs command modules, ORM migration
        // modules): a fixed set of named exports is consumed by directory /
        // filename convention at runtime, never through a static import. Each
        // convention fires only when its two gate names co-occur in this set, so
        // a module merely exporting one signature name is not blanket-exempted.
        let export_names: HashSet<&str> = exports.iter().map(|e| e.name.as_str()).collect();

        // Serverless function handler: a `handler` export in a file under a
        // `functions/` directory is invoked by the cloud runtime through the
        // deploy config's `handler: "functions/my-fn/index.handler"` string, not
        // by a static TS import, so it has no importer yet is live. Gated on the
        // `functions/` directory so an ordinary lone `handler` export elsewhere
        // is still flagged. Hoisted out of the loop — one path-segment scan.
        let in_serverless_functions_dir = is_in_serverless_functions_dir(&canon);

        // Node.js ESM customization-hooks module: a `.mjs`/`.mts` module loaded
        // by the Node runtime through `--loader`/`--import` (or `register(...)`),
        // which invokes its `resolve`/`load`/`globalPreload` exports by name,
        // never through a static import. Gated on the ESM file convention so an
        // ordinary `resolve`/`load` export in a `.ts`/`.js` file is not exempted;
        // the per-export shape gate below is the second, stronger half. Hoisted
        // out of the loop — one extension check.
        let is_node_loader_hook_file = is_node_loader_hook_file(&canon);

        // The two source scans below each tree-sitter-parse the whole file, so
        // they are computed lazily: only an export that already survived the
        // cheap index checks (almost none, in a healthy project) pays for them.
        //
        // `structurally_consumed`: types/interfaces consumed structurally by
        // other exported functions in the same file. Callers don't have to
        // import the type name — passing an object literal to the exported
        // function is enough — so the type's usage map looks empty but it is
        // not dead.
        //
        // `in_file_referenced`: names referenced anywhere in the file's body
        // (outside their own declaration site). Captures schema chains
        // (`BaseSchema.extend(...)`, `z.infer<typeof BaseSchema>`), object
        // composition, and any other intra-file re-use that doesn't go through
        // the import index.
        let mut structurally_consumed: Option<HashSet<String>> = None;
        let mut in_file_referenced: Option<HashSet<String>> = None;

        // Classes decorated with a custom-element-registering decorator
        // (`@customElement('tag')`). They are registered in the browser's
        // custom-element registry as a side effect and reached through their HTML
        // tag name, never a static import — so their export is live despite
        // having no importer. Computed lazily, like the scans above.
        let mut custom_element_classes: Option<HashSet<String>> = None;

        // Docusaurus `@site/` alias importers. The alias maps to the site root
        // via webpack and never resolves in the import index, so a component
        // consumed exclusively through `@site/src/...` would look dead. When an
        // unresolved `@site/`-aliased import plausibly references this file,
        // every export of it is live — the whole module is reachable.
        let mut site_alias_importer: Option<bool> = None;

        // Names of this file's symbols re-exported by an ng-packagr public-API
        // entry barrel (`lib.entryFile` of an `ng-package.json`). Such a symbol
        // is part of the Angular library's published surface, consumed
        // externally — the entry barrel re-exports it but no source file
        // imports it. Computed lazily: only an export that survived every cheap
        // check pays for the per-barrel scan.
        let mut ng_reexported: Option<HashSet<String>> = None;

        // Whether this module is a Cloudflare Worker module-format entry point —
        // an `export default` object carrying a `fetch`/`scheduled`/… lifecycle
        // handler. The Workers runtime resolves the entry from `wrangler.toml`
        // and invokes the default export's handlers by name, never through a
        // static import, so the `default` export (and any separately-exported
        // handler) is live despite having no importer. Computed lazily: only an
        // export named like a handler that survived every cheap check pays for
        // the parse.
        let mut cloudflare_worker_entry: Option<bool> = None;

        // Whether this module is an OXLint custom-plugin entry point — it imports
        // `definePlugin` from `@oxlint/plugins` and `export default`s a
        // `definePlugin(...)` call. OXLint resolves plugin modules from its
        // config and loads the default export at run time, never through a static
        // import, so the `default` export is live despite having no importer.
        // Computed lazily: only a surviving `default` export pays for the parse.
        let mut oxlint_plugin_entry: Option<bool> = None;

        // Whether this module is a k6 load-test script — it imports from the `k6`
        // runtime module (`k6` / `k6/*`) and has an `export default`. The k6 CLI
        // reads the `options` export and invokes `default`/`setup`/`teardown` by
        // name, never through a static import, so those exports are live despite
        // having no importer. Computed lazily: only a surviving export named like
        // a k6 magic export pays for the parse.
        let mut k6_script_entry: Option<bool> = None;

        // Names of this module's Convex backend exports — present only when the
        // module imports from `convex/server` (or `convex/_generated/server`) and
        // exposes a `defineSchema(...)` default or a `query`/`mutation`/`action`
        // (or internal-variant) wrapper-call export. The Convex deployment
        // registers each by path and the generated `api.*` types call them, never
        // through a static import, so they are live despite having no importer.
        // Computed lazily: only a surviving export pays for the parse, and the set
        // is scoped to the wrapper-call exports so a plain export stays flagged.
        let mut convex_magic_exports: Option<HashSet<String>> = None;

        // Names of this module's Node.js ESM loader-hook exports — the
        // `resolve`/`load`/`globalPreload` hooks declared with the canonical
        // chained-hook signature (`resolve`/`load`'s last parameter is the
        // `nextResolve`/`nextLoad` continuation). The Node runtime invokes them by
        // name through the `--loader`/`--import` machinery, never through a static
        // import, so they are live despite having no importer. Computed lazily and
        // only when the file already matched the ESM hooks convention, so a
        // non-`.mjs`/`.mts` module pays nothing and the set is scoped to the
        // shape-valid hook names so a plain `export const resolve` stays flagged.
        let mut node_loader_hook_exports: Option<HashSet<String>> = None;

        let mut diagnostics = Vec::new();
        for export in exports {
            if matches!(export.kind, ExportKind::StarReExport) {
                continue;
            }
            if magic.contains(export.name.as_str()) {
                continue;
            }
            if is_co_occurrence_exempt(&export.name, &export_names) {
                continue;
            }
            // Framework file-system-routing entry point (Next.js pages/App
            // Router, Remix/React Router routes, SvelteKit routes): the
            // convention's reserved exports are consumed by the router by name,
            // never through a static import. Path-convention-based, so it covers
            // monorepo route files whose framework dependency is invisible to
            // nearest-manifest detection.
            if is_framework_route_export(&canon, &export.name) {
                continue;
            }
            if in_serverless_functions_dir && export.name == SERVERLESS_HANDLER_EXPORT {
                continue;
            }
            if !index.get_usages(&canon, &export.name).is_empty() {
                continue;
            }
            // Cloudflare Worker entry: the `default` export (and any
            // separately-exported lifecycle handler) of a module whose
            // `export default` object carries a `fetch`/`scheduled`/… handler is
            // invoked by the Workers runtime by name, never imported. Gated on
            // the export being a handler name so an ordinary module pays nothing.
            if crate::project::CLOUDFLARE_WORKER_HANDLER_EXPORTS.contains(&export.name.as_str()) {
                let is_worker_entry = *cloudflare_worker_entry.get_or_insert_with(|| {
                    crate::project::is_cloudflare_worker_entry_source(ctx.source, ctx.lang)
                });
                if is_worker_entry {
                    continue;
                }
            }
            // OXLint custom plugin entry: the `default` export of a module that
            // imports `definePlugin` from `@oxlint/plugins` and exports
            // `definePlugin(...)` is loaded by OXLint from its config, never
            // imported. Gated on the export being `default` so an ordinary module
            // pays nothing.
            if crate::project::OXLINT_PLUGIN_ENTRY_EXPORTS.contains(&export.name.as_str()) {
                let is_plugin_entry = *oxlint_plugin_entry.get_or_insert_with(|| {
                    crate::project::is_oxlint_plugin_entry_source(ctx.source, ctx.lang)
                });
                if is_plugin_entry {
                    continue;
                }
            }
            // k6 load-test script: the `default` entry function, the `options`
            // runtime config, and the `setup`/`teardown` hooks of a module that
            // imports from `k6`/`k6/*` and has an `export default` are consumed by
            // the k6 CLI by name, never imported. Gated on the export being a k6
            // magic name so an ordinary module pays nothing.
            if crate::project::K6_SCRIPT_MAGIC_EXPORTS.contains(&export.name.as_str()) {
                let is_k6_entry = *k6_script_entry.get_or_insert_with(|| {
                    crate::project::is_k6_script_source(ctx.source, ctx.lang)
                });
                if is_k6_entry {
                    continue;
                }
            }
            // Convex backend function: a `defineSchema(...)` default or a
            // `query`/`mutation`/`action` (or internal-variant) wrapper-call
            // export of a module importing from `convex/server` is registered by
            // the Convex deployment and called via the generated `api.*` types,
            // never imported. Export names are arbitrary, so the exemption is the
            // precise set the source scan returns — a plain export stays flagged.
            let convex_magic = convex_magic_exports.get_or_insert_with(|| {
                crate::project::convex_magic_exports_for_source(ctx.source, ctx.lang)
            });
            if convex_magic.contains(export.name.as_str()) {
                continue;
            }
            // Node.js ESM loader hook: a `resolve`/`load`/`globalPreload` export
            // in an `.mjs`/`.mts` module whose declaration carries the canonical
            // chained-hook signature is invoked by the Node runtime through the
            // `--loader`/`--import` machinery, never imported. Gated on the ESM
            // file convention AND the shape-confirming scan, so an ordinary
            // `resolve`/`load` export does not match.
            if is_node_loader_hook_file
                && crate::project::NODE_LOADER_HOOK_EXPORTS.contains(&export.name.as_str())
            {
                let hooks = node_loader_hook_exports.get_or_insert_with(|| {
                    crate::project::node_loader_hook_exports_for_source(ctx.source, ctx.lang)
                });
                if hooks.contains(export.name.as_str()) {
                    continue;
                }
            }
            let structurally_consumed = structurally_consumed
                .get_or_insert_with(|| collect_structurally_consumed_types(ctx.source, ctx.lang));
            if structurally_consumed.contains(export.name.as_str()) {
                continue;
            }
            let in_file_referenced = in_file_referenced
                .get_or_insert_with(|| collect_in_file_referenced_names(ctx.source, ctx.lang));
            if in_file_referenced.contains(export.name.as_str()) {
                continue;
            }
            let custom_element_classes = custom_element_classes
                .get_or_insert_with(|| collect_custom_element_class_names(ctx.source, ctx.lang));
            if custom_element_classes.contains(export.name.as_str()) {
                continue;
            }
            let site_alias_importer = *site_alias_importer
                .get_or_insert_with(|| has_unresolved_site_alias_importer(index, &canon));
            if site_alias_importer {
                continue;
            }
            let ng_reexported = ng_reexported
                .get_or_insert_with(|| collect_ng_package_reexported_names(index, ctx.project, &canon));
            if ng_reexported.contains(export.name.as_str()) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: export.line,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!(
                    "export `{}` is never imported elsewhere in the project. \
                     Remove it or document why it's part of the public surface.",
                    export.name
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

/// Collect the names of types/interfaces that appear inside an exported
/// function signature in the same file. Such names are consumed
/// structurally — callers reach them by passing an object literal to the
/// exported function, never by importing the type — so they look unused in
/// the import index even though they're load-bearing.
///
/// The walk only inspects nodes within `export_statement` whose declaration
/// is a function (`function_declaration`, `generator_function_declaration`).
/// Inside those, every `type_identifier` is collected. Type identifiers
/// that appear inside another exported `type_alias_declaration` or
/// `interface_declaration` are deliberately ignored — chaining one
/// "potentially dead" type through another doesn't make either of them live.
fn collect_structurally_consumed_types(source: &str, lang: crate::files::Language) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(grammar) = ts_language_for(lang) else {
        return out;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return out;
    }
    let Some(tree) = parser.parse(source, None) else {
        return out;
    };
    let bytes = source.as_bytes();
    walk_tree(&tree, |node| {
        if node.kind() != "export_statement" {
            return;
        }
        for child in node.named_children(&mut node.walk()) {
            match child.kind() {
                "function_declaration" | "generator_function_declaration" => {
                    collect_type_identifiers(child, bytes, &mut out);
                }
                _ => {}
            }
        }
    });
    out
}

/// Collect names that occur 2+ times across the file's identifier and
/// type-identifier nodes at module top level (outside function bodies). The
/// declaration of an exported name contributes one occurrence; any additional
/// occurrence at top level means the name is consumed in-file by another
/// declaration (e.g. `BaseSchema.extend(...)`, `z.infer<typeof BaseSchema>`,
/// composition into another exported value).
///
/// Function bodies are excluded so that a type referenced only as a cast
/// inside an unrelated function (`{} as MyType`) does not silence the
/// diagnostic — see `still_flags_type_only_referenced_in_function_body`.
///
/// The heuristic deliberately ignores binding scope. A shadowed parameter
/// sharing a name with an export at top level would silence the diagnostic —
/// a false negative we accept in exchange for never re-flagging an export
/// that's genuinely re-used in the same file.
fn collect_in_file_referenced_names(source: &str, lang: crate::files::Language) -> HashSet<String> {
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let Some(grammar) = ts_language_for(lang) else {
        return HashSet::new();
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return HashSet::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return HashSet::new();
    };
    let bytes = source.as_bytes();
    let root = tree.root_node();
    let mut stack: Vec<tree_sitter::Node> = vec![root];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "identifier" | "type_identifier" | "shorthand_property_identifier" => {
                if let Ok(text) = node.utf8_text(bytes) {
                    *counts.entry(text.to_string()).or_insert(0) += 1;
                }
            }
            _ => {}
        }
        for child in node.named_children(&mut node.walk()) {
            match child.kind() {
                // Skip function bodies — references inside them aren't a sign
                // the exported name is consumed by another module-level export.
                "statement_block" => continue,
                // Skip export clauses (`export { Foo as Bar }`) — the
                // identifiers there are re-export references, not in-file
                // consumers. Counting them would inflate `Foo`'s occurrence
                // count and silence dead-export when neither `Foo` nor `Bar`
                // is imported elsewhere.
                "export_clause" | "export_specifier" => continue,
                _ => {}
            }
            stack.push(child);
        }
    }
    counts
        .into_iter()
        .filter_map(|(name, n)| if n >= 2 { Some(name) } else { None })
        .collect()
}

/// Push the text of every `type_identifier` in `node`'s signature into `out`.
/// Only descends into `formal_parameters` and `return_type` children; skips
/// `statement_block` so that type casts or local variable annotations inside
/// the function body do not silence dead-export for types that appear nowhere
/// in the public signature.
fn collect_type_identifiers(node: tree_sitter::Node, source: &[u8], out: &mut HashSet<String>) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "type_identifier" {
            if let Ok(text) = n.utf8_text(source) {
                out.insert(text.to_string());
            }
        }
        for child in n.named_children(&mut n.walk()) {
            if child.kind() == "statement_block" {
                continue;
            }
            stack.push(child);
        }
    }
}

/// Collect the names of classes decorated with a custom-element-registering
/// decorator (`@customElement('tag')`). Such a class is registered in the
/// browser's custom-element registry as a side effect of the decorator and is
/// reached through its HTML tag name, never through a static import — so its
/// export has no importer yet is live.
///
/// Walks every class declaration in the file. The decorators of an exported
/// decorated class (`@customElement('x') export class Foo`) attach to the
/// enclosing `export_statement`, while a non-exported one's attach to the
/// `class_declaration` itself; both placements are inspected. The decorator's
/// callee identifier is matched via the shared `is_custom_element_decorator_name`
/// predicate, so the registered tag string is irrelevant.
fn collect_custom_element_class_names(source: &str, lang: crate::files::Language) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(grammar) = ts_language_for(lang) else {
        return out;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return out;
    }
    let Some(tree) = parser.parse(source, None) else {
        return out;
    };
    let bytes = source.as_bytes();
    walk_tree(&tree, |node| {
        if node.kind() != "class_declaration" && node.kind() != "abstract_class_declaration" {
            return;
        }
        let Some(name) = node
            .named_children(&mut node.walk())
            .find(|c| c.kind() == "identifier" || c.kind() == "type_identifier")
            .and_then(|id| id.utf8_text(bytes).ok())
        else {
            return;
        };
        let on_class = node
            .named_children(&mut node.walk())
            .any(|c| is_custom_element_decorator(c, bytes));
        let on_export_parent = node
            .parent()
            .filter(|p| p.kind() == "export_statement")
            .is_some_and(|p| {
                p.named_children(&mut p.walk())
                    .any(|c| is_custom_element_decorator(c, bytes))
            });
        if on_class || on_export_parent {
            out.insert(name.to_string());
        }
    });
    out
}

/// True when `node` is a `decorator` whose callee identifier registers a custom
/// element. The callee is the `identifier` child of the decorator's
/// `call_expression` (`@customElement('x')`), or the bare `identifier` child of
/// the decorator (`@customElement`).
fn is_custom_element_decorator(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "decorator" {
        return false;
    }
    let callee = node
        .named_children(&mut node.walk())
        .find_map(|child| match child.kind() {
            "call_expression" => child
                .named_children(&mut child.walk())
                .find(|c| c.kind() == "identifier"),
            "identifier" => Some(child),
            _ => None,
        });
    callee
        .and_then(|id| id.utf8_text(source).ok())
        .is_some_and(is_custom_element_decorator_name)
}

/// True when `path` is listed as a CLI entry point in a `package.json`
/// `scripts` value (e.g. `"seed:dev": "bun run src/db/seed/dev.ts"`).
/// Compares the file's path relative to `project_root` (forward-slash,
/// no leading `./`) against the extracted `script_entry_files` list.
fn is_script_entry_point(
    path: &Path,
    project_root: Option<&Path>,
    script_entry_files: &[String],
) -> bool {
    if script_entry_files.is_empty() {
        return false;
    }
    let Some(root) = project_root else {
        return false;
    };
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    script_entry_files.iter().any(|entry| *entry == rel)
}

/// Entry points we deliberately never flag: `main.*` and `index.*` directly
/// at the project root. Nested `index.ts` files (e.g. barrel files in
/// feature folders) are expected to be imported and stay subject to the rule.
fn is_entry_point(path: &Path, project_root: Option<&Path>) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    if stem != "main" && stem != "index" {
        return false;
    }
    let Some(root) = project_root else {
        // No root detected (LSP / single-file) — err on the side of silence
        // for these conventional names.
        return true;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let canon_parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let canon_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    canon_parent == canon_root
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

    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> (TempDir, Vec<Diagnostic>) {
        run_on_project_with_pkg(None, files, target_rel)
    }

    fn run_on_project_with_pkg(
        package_json: Option<&str>,
        files: &[(&str, &str)],
        target_rel: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
        run_on_project_inner(package_json, Config::default(), files, target_rel)
    }

    fn run_on_project_with_entrypoints(
        entrypoints: Vec<String>,
        files: &[(&str, &str)],
        target_rel: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
        // A minimal package.json anchors project_root at the TempDir root so
        // entrypoints globs like "src/api/server.ts" resolve correctly.
        run_on_project_inner(
            Some(r#"{"name":"test"}"#),
            Config::with_entrypoints(entrypoints),
            files,
            target_rel,
        )
    }

    fn run_on_project_with_pkg_and_entrypoints(
        package_json: &str,
        entrypoints: Vec<String>,
        files: &[(&str, &str)],
        target_rel: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
        run_on_project_inner(
            Some(package_json),
            Config::with_entrypoints(entrypoints),
            files,
            target_rel,
        )
    }

    fn run_on_project_inner(
        package_json: Option<&str>,
        config: Config,
        files: &[(&str, &str)],
        target_rel: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        if let Some(package_json) = package_json {
            fs::write(dir.path().join("package.json"), package_json).unwrap();
        }
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
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
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
    fn no_fp_when_nearest_package_json_is_marker_only_issue_1823() {
        // Regression for #1823 (mswjs/msw) — `src/core/index.ts`'s nearest
        // manifest is a marker-only `/src/package.json` ({"type":"module"}) with
        // no `main`/`exports`, so library detection would read `is_library=false`
        // and flag every public export. The real library root sits above; the
        // marker is transparent, so the rule sees `is_library=true` and bails.
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"msw","main":"./lib/index.js","exports":{".":"./lib/index.js"}}"#,
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("src").join("core")).unwrap();
        fs::write(dir.path().join("src").join("package.json"), r#"{"type":"module"}"#).unwrap();
        fs::write(
            dir.path().join("src").join("core").join("index.ts"),
            "export const HttpResponse = 42;\n",
        )
        .unwrap();
        // A second source file so the scan is not in single-file mode.
        fs::write(dir.path().join("src").join("other.ts"), "export const z = 1;\n").unwrap();

        let config = Config::default();
        let source_files: Vec<SourceFile> = ["src/core/index.ts", "src/other.ts"]
            .iter()
            .map(|rel| SourceFile {
                path: dir.path().join(rel),
                language: Language::TypeScript,
            })
            .collect();
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let project = ProjectCtx::load(&refs, &config);

        let target_path = dir.path().join("src").join("core").join("index.ts");
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx,
            lang: Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        assert!(
            diags.is_empty(),
            "library root above a marker-only manifest must exempt exports, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_bazel_ng_package_reexported_symbol_issue_2299() {
        // Regression for #2299 (angular/angular) — in the monorepo a `@angular/*`
        // source package under `packages/<pkg>/` is built by Bazel's `ng_package`
        // rule, so its placeholder `package.json` declares no main/exports/module
        // and its `index.ts` barrel is not at the project root. A deep module's
        // public symbol (`Animation`) is consumed only through the barrel's
        // `export … from './src/animation'`; no source file imports it. The
        // sibling `BUILD.bazel` declaring `ng_package(...)` marks the barrel as the
        // package entry, so the re-exported symbol is live, not dead.
        let dir = TempDir::new().unwrap();
        // Monorepo root: a non-library marker so the package below is not at root.
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"angular-srcs","private":true}"#,
        )
        .unwrap();
        fs::write(dir.path().join("root.ts"), "export const root = 1;\n").unwrap();
        let pkg = dir.path().join("packages").join("animations");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(
            pkg.join("package.json"),
            r#"{"name":"@angular/animations","version":"0.0.0-PLACEHOLDER","dependencies":{"tslib":"^2.3.0"}}"#,
        )
        .unwrap();
        fs::write(
            pkg.join("BUILD.bazel"),
            "load(\"//tools:defaults.bzl\", \"ng_package\")\nng_package(\n    name = \"npm_package\",\n)\n",
        )
        .unwrap();
        fs::write(
            pkg.join("index.ts"),
            "export { Animation } from './src/animation';\n",
        )
        .unwrap();
        fs::write(
            pkg.join("src").join("animation.ts"),
            "export const Animation = 1;\n",
        )
        .unwrap();

        let config = Config::default();
        let rels = [
            "root.ts",
            "packages/animations/index.ts",
            "packages/animations/src/animation.ts",
        ];
        let source_files: Vec<SourceFile> = rels
            .iter()
            .map(|rel| SourceFile {
                path: dir.path().join(rel),
                language: Language::TypeScript,
            })
            .collect();
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let project = ProjectCtx::load(&refs, &config);

        let target_path = pkg.join("src").join("animation.ts");
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx,
            lang: Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        assert!(
            diags.is_empty(),
            "symbol re-exported by a Bazel ng_package barrel must not be flagged dead, got: {diags:?}"
        );
    }

    #[test]
    fn skips_in_single_file_scan_mode() {
        // Regression for rbaumier/comply#33 — `comply src/shared/foo.ts`
        // sees only one indexed file, so it can't see consumers and
        // every export would falsely look dead. Skip in that mode.
        let files: Vec<(&str, &str)> = vec![
            ("foo.ts", "export function foo() {}"),
        ];
        let (_dir, diags) = run_on_project(&files, "foo.ts");
        assert!(diags.is_empty(), "single-file scan must not run dead-export");
    }

    #[test]
    fn no_fp_for_custom_element_decorated_exported_class_issue_1805() {
        // Regression for #1805 (TanStack/virtual) — `@customElement('my-app')`
        // registers `MyApp` in the browser's custom-element registry as a side
        // effect; it is reached through the `<my-app>` HTML tag, never a static
        // import, so the import graph shows no importer even though it is live.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/main.ts",
                "import { customElement } from 'lit/decorators.js';\n\
                 @customElement('my-app')\n\
                 export class MyApp extends LitElement {}\n",
            ),
            ("src/other.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/main.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("MyApp")),
            "@customElement-decorated exported class must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_undecorated_unused_exported_class_issue_1805() {
        // Negative-space guard for #1805 — an exported class with no registering
        // decorator and no importer is genuinely dead and must still fire, even
        // alongside a custom-element class in a sibling file.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/widget.ts",
                "export class PlainWidget {}\n",
            ),
            (
                "src/main.ts",
                "import { customElement } from 'lit/decorators.js';\n\
                 @customElement('my-app')\n\
                 export class MyApp extends LitElement {}\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "undecorated unused exported class must still be flagged, got: {diags:?}"
        );
        assert!(diags[0].message.contains("PlainWidget"));
    }

    #[test]
    fn flags_export_with_no_importer() {
        let files: Vec<(&str, &str)> = vec![
            ("tax.ts", "export function computeTax() {}"),
            ("other.ts", "export const y = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "tax.ts");
        assert_eq!(diags.len(), 1, "computeTax is never imported");
        assert_eq!(diags[0].rule_id, "dead-export");
        assert!(
            diags[0].message.contains("computeTax"),
            "message should name the dead export, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn allows_exports_under_template_literal_dynamic_import_dir_issue_1789() {
        // Regression for #1789 (chakra-ui): `steps.tsx` lives under
        // `apps/compositions/src/`, loaded only via the template-literal dynamic
        // import in `example.tsx`. Its exports are live, never flagged dead.
        let files: Vec<(&str, &str)> = vec![
            (
                "apps/www/components/example.tsx",
                "import dynamic from 'next/dynamic';\n\
                 export const ExamplePreview = (props) => {\n\
                   const { name, scope = 'examples' } = props;\n\
                   return dynamic(() =>\n\
                     import(`../../compositions/src/${scope}/${name}`),\n\
                   );\n\
                 };\n",
            ),
            (
                "apps/compositions/src/ui/steps.tsx",
                "export const StepsRoot = 1;\nexport const StepsList = 2;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "apps/compositions/src/ui/steps.tsx");
        assert!(
            diags.is_empty(),
            "exports under a dynamic-import dir are live: {diags:?}"
        );
    }

    #[test]
    fn allows_export_imported_elsewhere() {
        let files: Vec<(&str, &str)> = vec![
            ("tax.ts", "export function computeTax() {}"),
            ("app.ts", "import { computeTax } from './tax';"),
        ];
        let (_dir, diags) = run_on_project(&files, "tax.ts");
        assert!(diags.is_empty(), "computeTax is imported, no diagnostic");
    }

    #[test]
    fn ignores_root_entry_points() {
        // `index.ts` at the project root acts as the entry — not flagged.
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export function bootstrap() {}"),
            ("other.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "index.ts");
        assert!(diags.is_empty(), "root index.ts must not be flagged");
    }

    #[test]
    fn ignores_test_files() {
        let files: Vec<(&str, &str)> = vec![
            ("tax.test.ts", "export function fixture() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "tax.test.ts");
        assert!(diags.is_empty(), "test files must not be flagged");
    }

    #[test]
    fn ignores_storybook_csf_story_files_issue_1666() {
        // Regression for #1666 — Storybook Component Story Format files
        // (`*.stories.tsx`) export Stories (named) and a meta object (default)
        // that the Storybook runtime discovers by glob, never through a static
        // import, so the import graph shows no importer. Every export is live.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/components/NotificationList.stories.tsx",
                "export default { component: NotificationList };\n\
                 export const SingleNotification = { args: {} };\n\
                 export const MultipleNotifications = { args: {} };\n\
                 export const InsufficientContrast = { args: {} };\n",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) =
            run_on_project(&files, "src/components/NotificationList.stories.tsx");
        assert!(
            diags.is_empty(),
            "Storybook CSF story exports must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_dead_export_in_ordinary_file_alongside_stories() {
        // Negative-space guard for #1666 — the Storybook skip is gated on the
        // `.stories.` filename, so a genuinely unused export in an ordinary
        // `.ts` file is still flagged.
        let files: Vec<(&str, &str)> = vec![
            ("src/lib/tax.ts", "export function computeTax() { return 0; }"),
            ("src/lib/other.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/lib/tax.ts");
        assert_eq!(
            diags.len(),
            1,
            "an ordinary unused export must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("computeTax"));
    }

    #[test]
    fn ignores_tanstack_router_lazy_file_imported_by_dash_prefixed_test() {
        // Regression for #78 — TanStack Router `.lazy.tsx` route exports a
        // component that's only consumed by a `-*.test.tsx` sibling. The
        // route file is a framework entry point, so dead-export must not
        // fire on its exports even if no other application file imports
        // them directly.
        let pkg = r#"{ "dependencies": { "@tanstack/react-router": "1.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/app/routes/_authed/index.lazy.tsx",
                "export function DashboardPage() { return null; }\n\
                 export const Route = createLazyFileRoute('/_authed/')({ component: DashboardPage });",
            ),
            (
                "src/app/routes/_authed/-index.test.tsx",
                "import { DashboardPage } from './index.lazy';\nDashboardPage;",
            ),
        ];
        let (_dir, diags) = run_on_project_with_pkg(
            Some(pkg),
            &files,
            "src/app/routes/_authed/index.lazy.tsx",
        );
        assert!(
            diags.is_empty(),
            ".lazy.tsx route is a framework entry; dead-export must not fire, got: {diags:?}"
        );
    }

    #[test]
    fn ignores_gatsby_ssr_lifecycle_exports_issue_1700() {
        // Regression for #1700 — Gatsby's `gatsby-ssr.js` at the project root is
        // a lifecycle entry: its named exports (`onRenderBody`, re-exported
        // `wrapPageElement`) are consumed by Gatsby's build pipeline, never by a
        // static import. The root-file match must bail out the whole file.
        let pkg = r#"{ "dependencies": { "gatsby": "5.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "gatsby-ssr.js",
                "export { wrapRootElement, wrapPageElement } from './gatsby-shared.js';\n\
                 export const onRenderBody = ({ setHtmlAttributes }) => { setHtmlAttributes({ lang: 'en' }); };\n",
            ),
            (
                "gatsby-shared.js",
                "export const wrapRootElement = ({ element }) => element;\n\
                 export const wrapPageElement = ({ element }) => element;\n",
            ),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "gatsby-ssr.js");
        assert!(
            diags.is_empty(),
            "gatsby-ssr.js is a framework lifecycle entry; dead-export must not fire, got: {diags:?}"
        );
    }

    #[test]
    fn ignores_nextjs_app_router_route_handler_exports_issue_1627() {
        // Regression for #1627 (shadcn-ui/ui) — a Next.js App Router `route.ts`
        // is a file-system-routed entry: its HTTP-method handlers (`GET`/`POST`/…)
        // are invoked by the Next.js runtime per request, and its segment-config
        // exports (`revalidate`, `dynamic`) plus `generateStaticParams` are read by
        // the build, never through a static import. The route-file match must bail
        // out the whole file.
        //
        // Configured entrypoints disable the `/app/` directory bail-out (the user
        // opted into checking backend-dir files), so only the per-file route match
        // protects this handler — the exact condition that surfaced the FP.
        let pkg = r#"{ "dependencies": { "next": "14.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "app/(app)/llm/[[...slug]]/route.ts",
                "import { NextResponse } from 'next/server';\n\
                 export const revalidate = false;\n\
                 export const dynamic = 'force-static';\n\
                 export async function GET() { return new NextResponse('x'); }\n\
                 export function generateStaticParams() { return []; }\n",
            ),
            ("app/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg_and_entrypoints(
            pkg,
            vec!["src/server.ts".to_string()],
            &files,
            "app/(app)/llm/[[...slug]]/route.ts",
        );
        assert!(
            diags.is_empty(),
            "Next.js App Router route.ts is a framework entry; dead-export must not fire, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_get_export_in_non_route_file_issue_1627() {
        // Negative-space guard for #1627 — the route-handler exemption is scoped to
        // the `route.ts` filename. A function named `GET` exported from an ordinary
        // module in the same Next.js project, with no importer, is genuinely dead
        // and must still fire: `GET` is not blanket-magic project-wide.
        let pkg = r#"{ "dependencies": { "next": "14.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            ("src/lib/http.ts", "export function GET() { return 0; }\n"),
            ("src/lib/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/lib/http.ts");
        assert_eq!(
            diags.len(),
            1,
            "a `GET` export in a non-route file must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("GET"));
    }

    #[test]
    fn ignores_framework_route_default_exports_without_detection_issue_1320() {
        // Regression for #1320 (cal.com) — Next.js page/API-route default exports
        // and App Router special-file exports are consumed by the file-system
        // router by name, never through a static import. In a monorepo the `next`
        // dependency can be invisible to nearest-manifest detection, so the
        // path-convention exemption must fire WITHOUT a detected framework.
        let files: Vec<(&str, &str)> = vec![
            ("pages/index.tsx", "export default function Index() { return null; }\n"),
            ("pages/api/x.ts", "export default function handler() {}\nexport {};\n"),
            ("app/dashboard/page.tsx", "export default function Page() { return null; }\n"),
            (
                "app/dashboard/route.ts",
                "export async function GET() { return new Response('ok'); }\n",
            ),
            ("src/other.ts", "export const z = 1;\nz;\n"),
        ];
        for target in [
            "pages/index.tsx",
            "pages/api/x.ts",
            "app/dashboard/page.tsx",
            "app/dashboard/route.ts",
        ] {
            let (_dir, diags) = run_on_project(&files, target);
            assert!(
                diags.is_empty(),
                "framework route file {target} must not be flagged dead, got: {diags:?}"
            );
        }
    }

    #[test]
    fn still_flags_dead_named_helper_in_pages_router_file_issue_1320() {
        // Negative-space guard for #1320 — the route exemption is scoped to the
        // convention's reserved exports. An ordinary `helper` export in a Pages
        // Router file, with no importer, is genuinely dead and must still fire.
        let files: Vec<(&str, &str)> = vec![
            (
                "pages/about.tsx",
                "export default function About() { return null; }\n\
                 export const helper = () => 1;\n",
            ),
            ("src/other.ts", "export const z = 1;\nz;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "pages/about.tsx");
        assert_eq!(
            diags.len(),
            1,
            "an ordinary unused export in a Pages Router file must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("helper"));
    }

    #[test]
    fn still_flags_dead_export_in_non_route_files_issue_1320() {
        // Negative-space guard for #1320 — the exemption is path-convention-gated.
        // A genuinely-unused `export const foo` in `src/utils.ts` and a non-route
        // `export default` in `src/lib/helper.ts` that nothing imports are both
        // still dead.
        let files: Vec<(&str, &str)> = vec![
            ("src/utils.ts", "export const foo = 1;\n"),
            ("src/lib/helper.ts", "export default function helper() {}\n"),
            ("src/other.ts", "export const z = 1;\nz;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/utils.ts");
        assert_eq!(diags.len(), 1, "unused src/utils.ts export is dead: {diags:?}");
        assert!(diags[0].message.contains("foo"));

        let (_dir, diags) = run_on_project(&files, "src/lib/helper.ts");
        assert_eq!(
            diags.len(),
            1,
            "non-route default export in src/lib/helper.ts is dead: {diags:?}"
        );
        assert!(diags[0].message.contains("default"));
    }

    #[test]
    fn ignores_remix_and_sveltekit_route_exports_without_detection_issue_1320() {
        // Regression for #1320 — Remix `routes/**` modules, the `root.tsx` app
        // root, and SvelteKit `+page`/`+server` route files expose reserved
        // exports the router consumes by name. The path-convention exemption
        // fires without a detected framework.
        let files: Vec<(&str, &str)> = vec![
            (
                "app/routes/dashboard.tsx",
                "export async function loader() { return {}; }\n\
                 export default function Dashboard() { return null; }\n",
            ),
            (
                "app/root.tsx",
                "export default function Root() { return null; }\n\
                 export function Layout() { return null; }\n",
            ),
            (
                "src/routes/+page.ts",
                "export const ssr = false;\nexport async function load() { return {}; }\n",
            ),
            (
                "src/routes/+server.ts",
                "export async function GET() { return new Response('ok'); }\n",
            ),
            ("src/other.ts", "export const z = 1;\nz;\n"),
        ];
        for target in [
            "app/routes/dashboard.tsx",
            "app/root.tsx",
            "src/routes/+page.ts",
            "src/routes/+server.ts",
        ] {
            let (_dir, diags) = run_on_project(&files, target);
            assert!(
                diags.is_empty(),
                "route file {target} must not be flagged dead, got: {diags:?}"
            );
        }
    }

    #[test]
    fn ignores_sveltekit_route_magic_exports_issue_1540() {
        // Regression for #1540 (immich-app/immich) — SvelteKit's reserved route
        // exports (`load`, `ssr`, `csr`) in a `+page.ts`/`+layout.ts` are consumed
        // by the file-system router by exact name, never through a static import,
        // so they have no importer yet are live framework entry points.
        let pkg = r#"{ "dependencies": { "@sveltejs/kit": "2.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/routes/+layout.ts",
                "export const ssr = false;\n\
                 export const csr = false;\n\
                 export async function load({ data }) { return data; }\n",
            ),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/routes/+layout.ts");
        assert!(
            diags.is_empty(),
            "SvelteKit route magic exports are framework-consumed: {diags:?}"
        );
    }

    #[test]
    fn ignores_sveltekit_param_matcher_export_issue_1540() {
        // Regression for #1540 — `match` in `src/params/*.ts` is the route
        // parameter matcher invoked by SvelteKit's router by convention.
        let pkg = r#"{ "dependencies": { "@sveltejs/kit": "2.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/params/integer.ts",
                "export function match(value) { return /^\\d+$/.test(value); }\n",
            ),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/params/integer.ts");
        assert!(
            diags.is_empty(),
            "SvelteKit param matcher `match` is framework-consumed: {diags:?}"
        );
    }

    #[test]
    fn still_flags_ordinary_export_in_sveltekit_route_file_issue_1540() {
        // Negative-space guard for #1540 — the exemption is scoped to SvelteKit's
        // reserved names. An ordinary `helper` export in the same route file, with
        // no importer, is genuinely dead and must still be flagged.
        let pkg = r#"{ "dependencies": { "@sveltejs/kit": "2.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/routes/+page.ts",
                "export async function load() { return {}; }\n\
                 export const helper = () => 1;\n",
            ),
            ("src/util.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/routes/+page.ts");
        assert_eq!(
            diags.len(),
            1,
            "an ordinary unused export in a route file must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("helper"));
    }

    #[test]
    fn still_flags_load_export_in_non_route_module_issue_1540() {
        // Negative-space guard for #1540 — `load` is a common generic name. A
        // `load` export from an ordinary module (not a `+page`/`+layout`/`+server`
        // route file), with no importer, is genuinely dead and must still fire:
        // the SvelteKit exemption is scoped to route files, not project-wide.
        let pkg = r#"{ "dependencies": { "@sveltejs/kit": "2.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            ("src/lib/data.ts", "export function load() { return {}; }\n"),
            ("src/lib/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/lib/data.ts");
        assert_eq!(
            diags.len(),
            1,
            "a `load` export in a non-route module must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("load"));
    }

    #[test]
    fn ignores_vitest_global_setup_exports_issue_1550() {
        // Regression for #1550 — a Vitest `globalSetup` module's `setup`/`teardown`
        // exports are invoked by the Vitest runtime by name (configured via
        // `test.globalSetup`), never through a static import, so they have no
        // importer yet are live framework entry points.
        let pkg = r#"{ "devDependencies": { "vitest": "^1.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "vitest.config.ts",
                "export default { test: { globalSetup: './global-setup.ts' } };\n",
            ),
            (
                "global-setup.ts",
                "export function setup() {}\nexport function teardown() {}\n",
            ),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "global-setup.ts");
        assert!(
            diags.is_empty(),
            "Vitest globalSetup `setup`/`teardown` are runtime-consumed: {diags:?}"
        );
    }

    #[test]
    fn still_flags_setup_export_in_non_global_setup_module_issue_1550() {
        // Negative-space guard for #1550 — `setup` is a common generic name. A
        // `setup` export from an ordinary module that no Vitest config references
        // as `globalSetup`, with no importer, is genuinely dead and must still
        // fire: the exemption is scoped to config-referenced globalSetup modules.
        let pkg = r#"{ "devDependencies": { "vitest": "^1.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "vitest.config.ts",
                "export default { test: { globalSetup: './global-setup.ts' } };\n",
            ),
            ("global-setup.ts", "export function setup() {}\n"),
            ("src/lib/helpers.ts", "export function setup() {}\n"),
            ("src/lib/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/lib/helpers.ts");
        assert_eq!(
            diags.len(),
            1,
            "a `setup` export in a non-globalSetup module must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("setup"));
    }

    #[test]
    fn ignores_cloudflare_worker_default_fetch_handler_issue_1561() {
        // Regression for #1561 (prisma bundle-size workers) — a Cloudflare Worker
        // module-format entry exports `default` an object with a `fetch` handler.
        // The Workers runtime resolves the entry from `wrangler.toml` and invokes
        // `fetch` by name; no JavaScript file imports it, so the default export
        // looks dead. It is a framework magic export — never flag it. The file is
        // a nested `index.js` (not at project root), so the root-entry bail-out
        // does not apply; the export-shape detection is what protects it.
        let files: Vec<(&str, &str)> = vec![
            (
                "packages/da-workers-pg/index.js",
                "import { PrismaPg } from '@prisma/adapter-pg';\n\
                 import { PrismaClient } from './client/edge';\n\
                 export default {\n\
                   async fetch(request, env) {\n\
                     const adapter = new PrismaPg({ connectionString: env.DATABASE_URL });\n\
                     const prisma = new PrismaClient({ adapter });\n\
                     const users = await prisma.user.findMany();\n\
                     return new Response(JSON.stringify(users));\n\
                   },\n\
                 };\n",
            ),
            ("packages/util.js", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "packages/da-workers-pg/index.js");
        assert!(
            diags.is_empty(),
            "Cloudflare Worker default-object fetch handler is runtime-consumed: {diags:?}"
        );
    }

    #[test]
    fn ignores_cloudflare_worker_scheduled_and_separately_exported_handler_issue_1561() {
        // #1561 coverage — the shape also matches a default object with shorthand
        // lifecycle handlers (`{ fetch, scheduled }`), and once a module is
        // recognized as a Worker entry its separately-exported handler functions
        // (`export async function scheduled`) are exempt too: the runtime may
        // invoke those by name as well.
        let files: Vec<(&str, &str)> = vec![
            (
                "workers/cron.ts",
                "export default { fetch, scheduled };\n\
                 export async function fetch(req) { return new Response('ok'); }\n\
                 export async function scheduled(event, env, ctx) {}\n",
            ),
            ("workers/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "workers/cron.ts");
        assert!(
            diags.is_empty(),
            "Worker entry default object + separately-exported handlers are live: {diags:?}"
        );
    }

    #[test]
    fn still_flags_ordinary_default_object_without_worker_handler_issue_1561() {
        // Negative-space guard for #1561 — the exemption is keyed on the Worker
        // export *shape* (default object with a lifecycle handler). An ordinary
        // `export default {}` with no `fetch`/`scheduled`/… handler, never
        // imported, is genuinely dead and must still be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/config.ts",
                "export default { name: 'app', version: 1 };\n",
            ),
            ("src/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/config.ts");
        assert_eq!(
            diags.len(),
            1,
            "an ordinary unused default object with no handler must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("default"));
    }

    #[test]
    fn still_flags_named_export_in_cloudflare_worker_entry_issue_1561() {
        // Negative-space guard for #1561 — only the `default` export and the
        // lifecycle-handler names are runtime-consumed. An ordinary named export
        // in a Worker entry module, with no importer, is genuinely dead and must
        // still be flagged: the exemption is scoped to the handler names.
        let files: Vec<(&str, &str)> = vec![
            (
                "workers/api.ts",
                "export default { async fetch(req) { return new Response('ok'); } };\n\
                 export const unusedHelper = () => 1;\n",
            ),
            ("workers/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "workers/api.ts");
        assert_eq!(
            diags.len(),
            1,
            "an ordinary named export in a Worker entry must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("unusedHelper"));
    }

    #[test]
    fn ignores_oxlint_plugin_default_export_issue_1557() {
        // Regression for #1557 (remix oxlint-plugins) — an OXLint custom plugin
        // imports `definePlugin` from `@oxlint/plugins` and `export default`s a
        // `definePlugin(...)` call. OXLint resolves plugin modules from its
        // config and loads the default export by itself; no JavaScript file
        // imports it, so the default export looks dead. The import-source +
        // call-shape detection is what protects it.
        let files: Vec<(&str, &str)> = vec![
            (
                "scripts/oxlint-plugins/interface-pascal-case-plugin.ts",
                "import { definePlugin, defineRule } from '@oxlint/plugins';\n\
                 export default definePlugin({\n\
                   name: 'interface-pascal-case',\n\
                   rules: {\n\
                     'interface-pascal-case': defineRule({ create() { return {}; } }),\n\
                   },\n\
                 });\n",
            ),
            ("scripts/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) =
            run_on_project(&files, "scripts/oxlint-plugins/interface-pascal-case-plugin.ts");
        assert!(
            diags.is_empty(),
            "OXLint plugin default export is loaded by the linter config at runtime: {diags:?}"
        );
    }

    #[test]
    fn still_flags_ordinary_default_export_without_define_plugin_issue_1557() {
        // Negative-space guard for #1557 — the exemption is keyed on the OXLint
        // plugin shape. An ordinary `export default {}` with no `definePlugin`
        // call and no `@oxlint/plugins` import, never imported, is genuinely dead
        // and must still be flagged.
        let files: Vec<(&str, &str)> = vec![
            ("src/config.ts", "export default { name: 'app' };\n"),
            ("src/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/config.ts");
        assert_eq!(
            diags.len(),
            1,
            "an ordinary unused default export must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("default"));
    }

    #[test]
    fn still_flags_define_plugin_default_without_oxlint_import_issue_1557() {
        // Negative-space guard for #1557 — both signals are required. A
        // `export default definePlugin(...)` whose `definePlugin` does NOT come
        // from `@oxlint/plugins` (here a local helper) is not the OXLint plugin
        // shape, so an unimported default export must still be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/widget.ts",
                "import { definePlugin } from './local-helpers';\n\
                 export default definePlugin({ name: 'widget' });\n",
            ),
            (
                "src/local-helpers.ts",
                "export function definePlugin(x) { return x; }\ndefinePlugin;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "a definePlugin default export without the @oxlint/plugins import must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("default"));
    }

    #[test]
    fn ignores_k6_script_magic_exports_issue_1530() {
        // Regression for #1530 (Grafana loadtest) — a k6 load-test script imports
        // from the `k6` runtime module and `export default`s its entry function.
        // The k6 CLI reads the `options` export and invokes
        // `default`/`setup`/`teardown` by name, never through a static import, so
        // those exports look dead. The import-source + export-default shape is what
        // protects them.
        let files: Vec<(&str, &str)> = vec![
            (
                "loadtest/script.ts",
                "import { sleep, check } from 'k6';\n\
                 import http from 'k6/http';\n\
                 export let options = { vus: 10, duration: '30s' };\n\
                 export const setup = () => {};\n\
                 export default (data) => { http.get('x'); sleep(1); };\n\
                 export const teardown = (data) => {};\n",
            ),
            ("loadtest/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "loadtest/script.ts");
        assert!(
            diags.is_empty(),
            "k6 script exports (options/setup/teardown/default) are consumed by the k6 runtime: {diags:?}"
        );
    }

    #[test]
    fn still_flags_options_export_in_non_k6_module_issue_1530() {
        // Negative-space guard for #1530 — the exemption is keyed on the k6 script
        // shape (a `k6`/`k6/*` import). An `export const options` in an ordinary
        // module with no k6 import, never imported, is genuinely dead and must
        // still be flagged.
        let files: Vec<(&str, &str)> = vec![
            ("src/config.ts", "export const options = { vus: 10 };\n"),
            ("src/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/config.ts");
        assert_eq!(
            diags.len(),
            1,
            "an `options` export in a non-k6 module must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("options"));
    }

    #[test]
    fn still_flags_k6_named_exports_without_default_issue_1530() {
        // Negative-space guard for #1530 — both signals are required. A module that
        // imports from `k6` but has no `export default` is not a k6 script shape, so
        // an unimported `options`/`setup` export must still be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/helpers.ts",
                "import { sleep } from 'k6';\n\
                 export const options = { vus: 1 };\n",
            ),
            ("src/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/helpers.ts");
        assert_eq!(
            diags.len(),
            1,
            "a k6-importing module without an export default is not a k6 script — its unused `options` export must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("options"));
    }

    #[test]
    fn ignores_convex_function_and_schema_exports_issue_1559() {
        // Regression for #1559 — Convex backend modules import their wrappers from
        // `convex/server` and export `query`/`mutation` calls (named, arbitrary
        // names) plus a `defineSchema(...)` default in `schema.ts`. The Convex
        // deployment registers each by path and the generated `api.*` types call
        // them, never through a static import, so they look dead. The
        // import-source + wrapper-call shape is what protects them.
        let files: Vec<(&str, &str)> = vec![
            (
                "convex/myFunctions.ts",
                "import { query, mutation } from 'convex/server';\n\
                 export const listNumbers = query({\n\
                   args: { count: v.number() },\n\
                   handler: async (ctx, args) => { return []; },\n\
                 });\n\
                 export const addNumber = mutation({\n\
                   args: { value: v.number() },\n\
                   handler: async (ctx, args) => {},\n\
                 });\n",
            ),
            (
                "convex/schema.ts",
                "import { defineSchema, defineTable } from 'convex/server';\n\
                 export default defineSchema({\n\
                   numbers: defineTable({ value: v.number(), userId: v.string() }),\n\
                 });\n",
            ),
            ("src/other.ts", "export const z = 1;\nz;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "convex/myFunctions.ts");
        assert!(
            diags.is_empty(),
            "Convex query/mutation exports are deployment-consumed: {diags:?}"
        );
        let (_dir, diags) = run_on_project(&files, "convex/schema.ts");
        assert!(
            diags.is_empty(),
            "Convex defineSchema default export is consumed by codegen: {diags:?}"
        );
    }

    #[test]
    fn still_flags_query_export_without_convex_import_issue_1559() {
        // Negative-space guard for #1559 — both signals are required. A module with
        // `export const x = query({...})` whose `query` is a local helper, not from
        // `convex/server`, is not a Convex module, so an unimported export must
        // still be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/data.ts",
                "import { query } from './local-query';\n\
                 export const listNumbers = query({ count: 1 });\n",
            ),
            (
                "src/local-query.ts",
                "export function query(x) { return x; }\nquery;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/data.ts");
        assert_eq!(
            diags.len(),
            1,
            "a query() export without the convex/server import must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("listNumbers"));
    }

    #[test]
    fn still_flags_plain_export_in_convex_module_issue_1559() {
        // Negative-space guard for #1559 — the exemption is scoped to the
        // wrapper-call exports. A plain `export const helper = 5` in a genuine
        // Convex module (not a `query`/`mutation`/`action` call) is not
        // deployment-consumed, so an unimported plain export must still be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "convex/myFunctions.ts",
                "import { query } from 'convex/server';\n\
                 export const listNumbers = query({ handler: async () => [] });\n\
                 export const helper = 5;\n",
            ),
            ("src/other.ts", "export const z = 1;\nz;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "convex/myFunctions.ts");
        assert_eq!(
            diags.len(),
            1,
            "a plain non-wrapper export in a Convex module must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("helper"));
    }

    #[test]
    fn ignores_remix_route_magic_exports_issue_1547() {
        // Regression for #1547 (triggerdotdev/trigger.dev) — Remix's reserved
        // route exports (`loader`, `action`, `meta`) in an `app/routes/*` module
        // are consumed by the file-system router by exact name, never through a
        // static import. With entrypoint globs configured (as trigger.dev has),
        // the framework-entry-dir bailout is skipped, so these would look dead.
        let pkg = r#"{ "dependencies": { "@remix-run/node": "2.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "app/routes/api.v1.projects.$projectRef.ts",
                "export async function loader() { return {}; }\n\
                 export async function action() { return {}; }\n\
                 export function meta() { return [{ title: \"x\" }]; }\n",
            ),
            ("app/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg_and_entrypoints(
            pkg,
            vec!["app/util.ts".to_string()],
            &files,
            "app/routes/api.v1.projects.$projectRef.ts",
        );
        assert!(
            diags.is_empty(),
            "Remix route magic exports are framework-consumed: {diags:?}"
        );
    }

    #[test]
    fn still_flags_ordinary_export_in_remix_route_file_issue_1547() {
        // Negative-space guard for #1547 — the exemption is scoped to Remix's
        // reserved names. An ordinary `helper` export in the same route module,
        // with no importer, is genuinely dead and must still be flagged.
        let pkg = r#"{ "dependencies": { "@remix-run/node": "2.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "app/routes/dashboard.tsx",
                "export async function loader() { return {}; }\n\
                 export const helper = () => 1;\n",
            ),
            ("app/util.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg_and_entrypoints(
            pkg,
            vec!["app/util.ts".to_string()],
            &files,
            "app/routes/dashboard.tsx",
        );
        assert_eq!(
            diags.len(),
            1,
            "an ordinary unused export in a route module must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("helper"));
    }

    #[test]
    fn still_flags_loader_export_in_non_route_remix_module_issue_1547() {
        // Negative-space guard for #1547 — `loader`/`meta`/`action` are common
        // generic names. A `loader` export from an ordinary module (not under
        // `app/routes/`), with no importer, is genuinely dead and must still
        // fire: the Remix exemption is scoped to route modules, not project-wide.
        let pkg = r#"{ "dependencies": { "@remix-run/node": "2.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            ("app/lib/data.ts", "export function loader() { return {}; }\n"),
            ("app/lib/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "app/lib/data.ts");
        assert_eq!(
            diags.len(),
            1,
            "a `loader` export in a non-route module must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("loader"));
    }

    #[test]
    fn ignores_gatsby_node_api_exports_issue_1700() {
        // Regression for #1700 — `gatsby-node.js` named exports (`createPages`,
        // `onCreateNode`) are Gatsby Node APIs invoked by the build, not imported.
        let pkg = r#"{ "dependencies": { "gatsby": "5.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "gatsby-node.js",
                "export const createPages = async ({ actions }) => { actions.createPage({}); };\n\
                 export const onCreateNode = ({ node }) => node;\n",
            ),
            ("src/util.js", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "gatsby-node.js");
        assert!(
            diags.is_empty(),
            "gatsby-node.js Node-API exports are framework-consumed: {diags:?}"
        );
    }

    #[test]
    fn ignores_gatsby_page_default_and_head_exports_issue_1700() {
        // Regression for #1700 — files under `src/pages/` are consumed by
        // Gatsby's file-system router: the default export is the page component
        // and `Head` is the Gatsby v5 Head API. Neither is imported by user code.
        let pkg = r#"{ "dependencies": { "gatsby": "5.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/pages/index.js",
                "export default function IndexPage() { return null; }\n\
                 export function Head() { return null; }\n",
            ),
            ("src/util.js", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/pages/index.js");
        assert!(
            diags.is_empty(),
            "Gatsby src/pages/* exports are router-consumed entry points: {diags:?}"
        );
    }

    #[test]
    fn flags_ordinary_dead_export_in_gatsby_project_issue_1700() {
        // Negative-space guard for #1700 — a genuinely unused export in an
        // ordinary source file (not a Gatsby entry/page) is still flagged even
        // when the project is detected as Gatsby.
        let pkg = r#"{ "dependencies": { "gatsby": "5.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            ("src/lib/tax.js", "export function computeTax() { return 0; }\n"),
            ("src/lib/other.js", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/lib/tax.js");
        assert_eq!(
            diags.len(),
            1,
            "an ordinary unused export must still be flagged in a Gatsby project: {diags:?}"
        );
        assert!(diags[0].message.contains("computeTax"));
    }

    #[test]
    fn ignores_astro_middleware_on_request_issue_1807() {
        // Regression for #1807 (clerk/javascript) — Astro's `src/middleware.ts`
        // must export `onRequest`, invoked by the request pipeline by convention,
        // never through a static import, so it has no importer yet is live.
        let pkg = r#"{ "dependencies": { "astro": "4.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/middleware.ts",
                "export const onRequest = (context, next) => next();\n",
            ),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/middleware.ts");
        assert!(
            diags.is_empty(),
            "Astro middleware `onRequest` is a framework entry: {diags:?}"
        );
    }

    #[test]
    fn ignores_astro_page_exports_issue_1807() {
        // Regression for #1807 — files under `src/pages/` are consumed by Astro's
        // file-system router: the default component export, HTTP method handlers
        // in API routes, and `getStaticPaths`/`prerender` directives are never
        // imported by user code.
        let pkg = r#"{ "dependencies": { "astro": "4.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/pages/api/me.ts",
                "export const GET = async ({ locals }) => new Response(null);\n\
                 export const prerender = false;\n",
            ),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/pages/api/me.ts");
        assert!(
            diags.is_empty(),
            "Astro src/pages/* exports are router-consumed entry points: {diags:?}"
        );
    }

    #[test]
    fn ignores_astro_page_route_magic_exports_with_entrypoints_issue_1807() {
        // Regression for #1807 — when entrypoint globs are configured the
        // framework-entry-dir bailout is skipped, so a page's reserved exports
        // (`getStaticPaths`, HTTP method handlers) would look dead. They are
        // route-scoped magic exports and stay live in `/pages/` files.
        let pkg = r#"{ "dependencies": { "astro": "4.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/pages/blog/[slug].ts",
                "export async function getStaticPaths() { return []; }\n\
                 export const GET = async () => new Response(null);\n",
            ),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg_and_entrypoints(
            pkg,
            vec!["src/util.ts".to_string()],
            &files,
            "src/pages/blog/[slug].ts",
        );
        assert!(
            diags.is_empty(),
            "Astro page route magic exports are framework-consumed: {diags:?}"
        );
    }

    #[test]
    fn still_flags_on_request_export_in_non_astro_project_issue_1807() {
        // Negative-space guard for #1807 — the Astro middleware exemption is gated
        // on the `astro` dependency. The same `onRequest` shape in a `middleware.ts`
        // of a non-Astro project, with no importer, is genuinely dead and fires.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/middleware.ts",
                "export const onRequest = (context, next) => next();\n",
            ),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/middleware.ts");
        assert_eq!(
            diags.len(),
            1,
            "an `onRequest` export in a non-Astro project must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("onRequest"));
    }

    #[test]
    fn still_flags_get_export_in_non_page_astro_module_issue_1807() {
        // Negative-space guard for #1807 — the HTTP-handler exemption is scoped to
        // `/pages/` files. A `GET` export from an ordinary module (not under
        // `src/pages/`) in an Astro project, with no importer, is genuinely dead.
        let pkg = r#"{ "dependencies": { "astro": "4.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            ("src/lib/http.ts", "export function GET() { return 0; }\n"),
            ("src/lib/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/lib/http.ts");
        assert_eq!(
            diags.len(),
            1,
            "a `GET` export in a non-page module must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("GET"));
    }

    #[test]
    fn still_flags_ordinary_export_in_astro_page_file_issue_1807() {
        // Negative-space guard for #1807 — the page exemption is scoped to Astro's
        // reserved names. An ordinary `helper` export in the same page file, with
        // no importer, is genuinely dead and must still be flagged.
        let pkg = r#"{ "dependencies": { "astro": "4.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/pages/blog/[slug].ts",
                "export async function getStaticPaths() { return []; }\n\
                 export const helper = () => 1;\n",
            ),
            ("src/util.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg_and_entrypoints(
            pkg,
            vec!["src/util.ts".to_string()],
            &files,
            "src/pages/blog/[slug].ts",
        );
        assert_eq!(
            diags.len(),
            1,
            "an ordinary unused export in an Astro page file must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("helper"));
    }

    #[test]
    fn ignores_module_consumed_via_namespace_import() {
        // When `import * as ns from './m'` exists, individual symbol usages
        // are intentionally not linked; flagging every export would be noise.
        let files: Vec<(&str, &str)> = vec![
            ("m.ts", "export const a = 1; export const b = 2;"),
            ("app.ts", "import * as ns from './m';"),
        ];
        let (_dir, diags) = run_on_project(&files, "m.ts");
        assert!(
            diags.is_empty(),
            "namespace importer suppresses dead-export"
        );
    }

    #[test]
    fn flags_multiple_dead_exports_independently() {
        let files: Vec<(&str, &str)> = vec![
            ("m.ts", "export const a = 1;\nexport const b = 2;"),
            ("app.ts", "import { a } from './m';"),
        ];
        let (_dir, diags) = run_on_project(&files, "m.ts");
        assert_eq!(diags.len(), 1, "only `b` should be flagged");
        assert!(diags[0].message.contains('b'));
    }

    #[test]
    fn ignores_components_ui_directory() {
        // shadcn convention: drop-in components installed before any importer
        // exists must not be flagged.
        let files: Vec<(&str, &str)> = vec![
            ("components/ui/button.tsx", "export function Button() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "components/ui/button.tsx");
        assert!(
            diags.is_empty(),
            "components/ui/* should be skipped: {diags:?}"
        );
    }

    #[test]
    fn ignores_src_components_ui_directory() {
        let files: Vec<(&str, &str)> = vec![
            ("src/components/ui/card.tsx", "export function Card() {}"),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/components/ui/card.tsx");
        assert!(
            diags.is_empty(),
            "src/components/ui/* should be skipped: {diags:?}"
        );
    }

    #[test]
    fn ignores_lib_ui_directory() {
        let files: Vec<(&str, &str)> = vec![
            ("lib/ui/avatar.tsx", "export function Avatar() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "lib/ui/avatar.tsx");
        assert!(diags.is_empty(), "lib/ui/* should be skipped: {diags:?}");
    }

    #[test]
    fn ignores_generated_files() {
        let files: Vec<(&str, &str)> = vec![
            (
                "schema.ts",
                "// @generated by codegen. do not edit.\nexport const TableA = {};",
            ),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "schema.ts");
        assert!(
            diags.is_empty(),
            "@generated files should be skipped: {diags:?}"
        );
    }

    #[test]
    fn ignores_block_comment_generated_marker() {
        let files: Vec<(&str, &str)> = vec![
            ("schema.ts", "/* @generated */\nexport const Settings = {};"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "schema.ts");
        assert!(
            diags.is_empty(),
            "/* @generated */ should be skipped: {diags:?}"
        );
    }

    #[test]
    fn no_crash_on_multibyte_generated_scan() {
        let files: Vec<(&str, &str)> = vec![
            ("tax.ts", "// مثال عربي\nexport function computeTax() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "tax.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_components_outside_ui_dir() {
        let files: Vec<(&str, &str)> = vec![
            (
                "components/feature/header.tsx",
                "export function Header() {}",
            ),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "components/feature/header.tsx");
        assert_eq!(
            diags.len(),
            1,
            "components/<feature>/ should still be flagged"
        );
    }

    #[test]
    fn skips_type_used_in_exported_function_signature() {
        // Regression for #100 — `FormServerErrorTarget` parameterizes
        // `applyProblemErrorToForm`'s second argument. Callers pass an
        // object literal into the function and never import the type by
        // name, so the import index sees zero usages. The type IS still
        // consumed structurally; dead-export must keep quiet.
        let files: Vec<(&str, &str)> = vec![
            (
                "form-server-errors.ts",
                "export type FormServerErrorTarget = { field: string };\n\
                 export function applyProblemErrorToForm(error: Error, target: FormServerErrorTarget): void {}\n",
            ),
            (
                "app.ts",
                "import { applyProblemErrorToForm } from './form-server-errors';\n\
                 applyProblemErrorToForm(new Error('x'), { field: 'email' });\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "form-server-errors.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("FormServerErrorTarget")),
            "type used structurally by an exported function must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_type_not_referenced_by_any_export() {
        // Sibling guard for #100 — a truly orphan type with no importer
        // and no in-file consumer must still be flagged.
        let files: Vec<(&str, &str)> = vec![
            ("types.ts", "export type Orphan = { a: number };\n"),
            ("other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "types.ts");
        assert_eq!(diags.len(), 1, "orphan type should still be flagged");
        assert!(diags[0].message.contains("Orphan"));
    }

    #[test]
    fn still_flags_type_only_referenced_in_function_body() {
        // Regression — a type that appears only as a cast (`as MyType`) inside
        // a function body, not in the function's signature, must still be
        // flagged as dead. Previously `collect_type_identifiers` walked all
        // descendants including `statement_block`, which caused the body cast
        // to silently suppress the diagnostic.
        let files: Vec<(&str, &str)> = vec![
            (
                "casts.ts",
                "export type BodyOnly = { x: number };\n\
                 export function doStuff() {\n\
                   const v = {} as BodyOnly;\n\
                   return v;\n\
                 }\n",
            ),
            ("other.ts", "import { doStuff } from './casts';\ndoStuff();\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "casts.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("BodyOnly")),
            "type only cast inside body should still be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn skips_schema_reused_via_extend_in_same_file() {
        // Regression for #95 — `TeamCentralCodeSchema` is consumed in-file by
        // `TeamCentralCodeSchema.extend(...)` and `z.infer<typeof TeamCentralCodeSchema>`.
        // Only the derived schema is imported elsewhere; dead-export must not
        // flag the base.
        let files: Vec<(&str, &str)> = vec![
            (
                "schemas.ts",
                "import { z } from 'zod';\n\
                 export const TeamCentralCodeSchema = z.object({ code: z.string() });\n\
                 export type TeamCentralCode = z.infer<typeof TeamCentralCodeSchema>;\n\
                 export const TeamCentralCodeWithCentraleResponseSchema = TeamCentralCodeSchema.extend({ extra: z.string() });\n\
                 export type TeamCentralCodeWithCentraleResponse = z.infer<typeof TeamCentralCodeWithCentraleResponseSchema>;\n",
            ),
            (
                "app.ts",
                "import { TeamCentralCodeWithCentraleResponseSchema } from './schemas';\n\
                 TeamCentralCodeWithCentraleResponseSchema.parse({});\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "schemas.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("TeamCentralCodeSchema")),
            "base schema reused in-file via .extend / z.infer<typeof> must not be flagged, got: {diags:?}"
        );
        assert!(
            diags.iter().all(|d| !d.message.contains("TeamCentralCode\"")),
            "base type reused in-file must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_export_only_re_exported_via_alias() {
        // Regression — `export { Foo as Bar }` used to inflate `Foo`'s
        // in-file reference count to 2, silencing dead-export even when
        // neither `Foo` nor `Bar` is imported by any other file.
        let files: Vec<(&str, &str)> = vec![
            (
                "reexport.ts",
                "export const Foo = 1;\nexport { Foo as Bar };\n",
            ),
            ("other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "reexport.ts");
        let names: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(
            diags.iter().any(|d| d.message.contains("`Foo`")),
            "Foo is never imported — should be flagged, got: {names:?}"
        );
        assert!(
            diags.iter().any(|d| d.message.contains("`Bar`")),
            "Bar is never imported — should be flagged, got: {names:?}"
        );
    }

    #[test]
    fn ignores_tanstack_router_non_lazy_route_file_with_dollar_params() {
        // Regression for #382 — `users.$userId.tsx` in a `/routes/` directory
        // is a TanStack Router file-based route. Its `Route` export is a magic
        // export consumed by the router tree, not imported by application
        // files. dead-export must not fire.
        let pkg = r#"{ "dependencies": { "@tanstack/react-router": "1.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/app/routes/users.$userId.tsx",
                "export const Route = createLazyFileRoute('/users/$userId')({});",
            ),
            (
                "src/generated/routeTree.ts",
                "// @generated by @tanstack/router-cli\nexport const routeTree = {};",
            ),
            (
                "src/app/routes/-users.$userId.test.tsx",
                "import { UsersUserIdRoute } from '../../generated/routeTree';\nconst r = UsersUserIdRoute;",
            ),
        ];
        let (_dir, diags) = run_on_project_with_pkg(
            Some(pkg),
            &files,
            "src/app/routes/users.$userId.tsx",
        );
        assert!(
            diags.is_empty(),
            "route file in /routes/ is a framework entry — dead-export must not fire: {diags:?}"
        );
    }

    #[test]
    fn ignores_tanstack_start_router_factory_export_issue_495() {
        // Regression for #495 — TanStack Start's `getRouter`/`createRouter`
        // factory in `router.tsx` is consumed only by the gitignored
        // `routeTree.gen.ts` (via `import type { getRouter }` and the
        // `Register` interface). That file is absent from the index, so the
        // export looks dead. It's a framework magic export — never flag it.
        let pkg = r#"{ "dependencies": { "@tanstack/react-start": "1.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/app/router.tsx",
                "export const getRouter = (() => (): Router => buildRouter())();",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/app/router.tsx");
        assert!(
            diags.iter().all(|d| !d.message.contains("getRouter")),
            "TanStack Start router factory must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn ignores_framework_entry_file_names() {
        let files: Vec<(&str, &str)> = vec![
            ("src/routeTree.gen.ts", "export const routeTree = {};"),
            ("src/app.ts", "export const z = 1;"),
        ];
        let pkg = r#"{"dependencies":{"@tanstack/react-router":"1"}}"#;
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/routeTree.gen.ts");
        assert!(
            diags.is_empty(),
            "generated TanStack route tree should be a framework entry point: {diags:?}"
        );
    }

    // Regression tests for issue #446

    #[test]
    fn no_fp_for_factory_in_test_data_dir() {
        // Regression for #1395 — immich's `web/src/test-data/factories/*`
        // factories are imported only from `.spec.ts` files through the
        // SvelteKit `@test-data/*` alias. That alias lives in the generated
        // (gitignored) `.svelte-kit/tsconfig.json`, so the import index can't
        // resolve it and every factory export looks dead. Files under a
        // `test-data/` directory are fixtures consumed from tests — skip them.
        let files: Vec<(&str, &str)> = vec![
            (
                "web/src/test-data/factories/user-factory.ts",
                "export const userFactory = createUserResponseDto();\n\
                 export const userAdminFactory = createUserResponseDto();",
            ),
            (
                // Imports the factory via an unresolvable alias, mirroring the
                // SvelteKit `@test-data/*` setup. The index drops the import,
                // leaving the factory exports with zero recorded usages.
                "web/src/lib/utils.spec.ts",
                "import { userFactory } from '@test-data/factories/user-factory';\nuserFactory;",
            ),
            ("web/src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "web/src/test-data/factories/user-factory.ts");
        assert!(
            diags.is_empty(),
            "factories under test-data/ must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_export_consumed_by_test_file() {
        // Regression for #446 — `renderWithProviders` is exported from a
        // test-helpers file that is NOT itself a test file (no `.test.` in name,
        // not in a `__tests__/` dir). It is imported by test files; dead-export
        // must not fire because test files ARE part of the import graph.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/app/test-helpers/index.ts",
                "export function renderWithProviders() {}",
            ),
            (
                "src/features/user/user.test.ts",
                "import { renderWithProviders } from '../../app/test-helpers';\nrenderWithProviders();",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/app/test-helpers/index.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("renderWithProviders")),
            "test-helper export consumed by test file must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_package_json_script_entry_point() {
        // Regression for #446 — `seedDevData` is exported from a file that is
        // invoked as a CLI entry point via a package.json script
        // (`"seed:dev": "bun run src/db/seed/dev.ts"`). No TS file imports it.
        // The file path matches the script entry point pattern, so dead-export
        // must not fire.
        let pkg = r#"{
            "scripts": {
                "seed:dev": "bun run src/db/seed/dev.ts",
                "delete-user": "bun run src/scripts/deleteUser.ts"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/db/seed/dev.ts",
                "export async function seedDevData(): Promise<void> {}",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/db/seed/dev.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("seedDevData")),
            "CLI entry point export must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_second_package_json_script_entry_point() {
        // Regression for #446 — another script entry point
        let pkg = r#"{
            "scripts": {
                "delete-user": "bun run src/scripts/deleteUser.ts"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/scripts/deleteUser.ts",
                "export async function deleteUser(id: string): Promise<void> {}",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) =
            run_on_project_with_pkg(Some(pkg), &files, "src/scripts/deleteUser.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("deleteUser")),
            "CLI entry point export must not be flagged, got: {diags:?}"
        );
    }

    // Regression tests for issue #754 — backend dead exports not flagged

    #[test]
    fn flags_dead_export_in_backend_api_schema_when_entrypoints_configured() {
        // Regression for #754 — dead export in src/api/** was silenced by the
        // framework entry-dirs bail-out even though it has zero importers.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/api/features/x/schema/x.ts",
                "export const __DEAD = \"x\";",
            ),
            ("src/api/server.ts", "export const app = {};"),
            ("src/app/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project_with_entrypoints(
            vec!["src/api/server.ts".to_string()],
            &files,
            "src/api/features/x/schema/x.ts",
        );
        assert!(
            diags.iter().any(|d| d.message.contains("__DEAD")),
            "dead export in backend schema must be flagged when entrypoints configured, got: {diags:?}"
        );
    }

    #[test]
    fn flags_dead_export_in_backend_handler_when_entrypoints_configured() {
        let files: Vec<(&str, &str)> = vec![
            (
                "src/api/features/x/list-x.ts",
                "export const __DEAD = \"x\";",
            ),
            ("src/api/server.ts", "export const app = {};"),
            ("src/app/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project_with_entrypoints(
            vec!["src/api/server.ts".to_string()],
            &files,
            "src/api/features/x/list-x.ts",
        );
        assert!(
            diags.iter().any(|d| d.message.contains("__DEAD")),
            "dead export in backend handler must be flagged when entrypoints configured, got: {diags:?}"
        );
    }

    #[test]
    fn does_not_flag_entrypoint_file_itself() {
        // The file listed in entrypoints is the entry — never flagged.
        let files: Vec<(&str, &str)> = vec![
            ("src/api/server.ts", "export const __DEAD = \"x\";"),
            ("src/app/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project_with_entrypoints(
            vec!["src/api/server.ts".to_string()],
            &files,
            "src/api/server.ts",
        );
        assert!(
            diags.is_empty(),
            "entrypoint file itself must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn does_not_flag_backend_export_imported_by_another_file() {
        // A backend export that IS imported — not flagged.
        let files: Vec<(&str, &str)> = vec![
            ("src/api/features/x/schema.ts", "export const UsedSchema = {};"),
            (
                "src/api/features/x/handler.ts",
                "import { UsedSchema } from './schema';",
            ),
            ("src/api/server.ts", "export const app = {};"),
        ];
        let (_dir, diags) = run_on_project_with_entrypoints(
            vec!["src/api/server.ts".to_string()],
            &files,
            "src/api/features/x/schema.ts",
        );
        assert!(
            diags.is_empty(),
            "imported backend export must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn frontend_behavior_unchanged_when_entrypoints_configured() {
        // Frontend dead exports are still flagged when entrypoints is set.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/app/features/x/head.ts",
                "export const __DEAD = \"x\";",
            ),
            ("src/api/server.ts", "export const app = {};"),
            ("src/app/other.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project_with_entrypoints(
            vec!["src/api/server.ts".to_string()],
            &files,
            "src/app/features/x/head.ts",
        );
        assert!(
            diags.iter().any(|d| d.message.contains("__DEAD")),
            "frontend dead export must still be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_yargs_command_module() {
        // Regression for #1417 — redwood's CLI command modules export the
        // yargs command shape (`command`/`description`/`builder`/`handler`).
        // yargs loads these via `commandDir()` / `.command(require(...))`, so
        // there is no static importer; dead-export must not flag them.
        let files: Vec<(&str, &str)> = vec![
            (
                "packages/cli/src/commands/destroy/page/page.js",
                "export const command = 'page <name> [path]'\n\
                 export const description = 'Destroy a page and route component'\n\
                 export const builder = (yargs) => {}\n\
                 export const handler = async ({ name, path }) => {}\n",
            ),
            ("packages/cli/src/index.js", "export const z = 1;"),
        ];
        let (_dir, diags) =
            run_on_project(&files, "packages/cli/src/commands/destroy/page/page.js");
        assert!(
            diags.is_empty(),
            "yargs command module exports must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_ordinary_handler_export() {
        // Sibling guard for #1417 — a module that merely exports a `handler`
        // (no co-occurring `command`) is not a yargs command module and must
        // still be flagged when no importer references it.
        let files: Vec<(&str, &str)> = vec![
            ("handler.ts", "export const handler = () => {};"),
            ("other.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "handler.ts");
        assert_eq!(
            diags.len(),
            1,
            "lone handler export (no command) must still be flagged, got: {diags:?}"
        );
        assert!(diags[0].message.contains("handler"));
    }

    #[test]
    fn no_fp_for_migration_module() {
        // Regression for #1421 — immich's Kysely migration modules export the
        // canonical `up`/`down` signature. The migration runner discovers them
        // by directory convention and calls them at runtime, so there is no
        // static importer; dead-export must not flag them.
        let files: Vec<(&str, &str)> = vec![
            (
                "server/src/schema/migrations/1746768490606-AddUserPincode.ts",
                "import { Kysely, sql } from 'kysely';\n\
                 export async function up(db: Kysely<any>): Promise<void> {\n\
                   await sql`ALTER TABLE \"users\" ADD \"pinCode\" character varying;`.execute(db);\n\
                 }\n\
                 export async function down(db: Kysely<any>): Promise<void> {\n\
                   await sql`ALTER TABLE \"users\" DROP COLUMN \"pinCode\";`.execute(db);\n\
                 }\n",
            ),
            ("server/src/index.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(
            &files,
            "server/src/schema/migrations/1746768490606-AddUserPincode.ts",
        );
        assert!(
            diags.is_empty(),
            "migration module up/down exports must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_lone_up_export() {
        // Sibling guard for #1421 — a module that exports only `up` (no
        // co-occurring `down`) does not have the migration shape and must still
        // be flagged when no importer references it.
        let files: Vec<(&str, &str)> = vec![
            ("up.ts", "export const up = () => {};"),
            ("other.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "up.ts");
        assert_eq!(
            diags.len(),
            1,
            "lone up export (no down) must still be flagged, got: {diags:?}"
        );
        assert!(diags[0].message.contains("up"));
    }

    #[test]
    fn yargs_and_migration_share_co_occurrence_mechanism() {
        // The single declarative `CO_OCCURRENCE_EXEMPTIONS` table services BOTH
        // the yargs and the migration convention through one
        // `is_co_occurrence_exempt` check — no per-convention Rust predicate. A
        // migration file (up+down) and a yargs file (command+handler) are both
        // exempt, while a lone `up` (gate not satisfied) is still flagged,
        // proving the gate is the declarative `all(gate)` check, not a
        // hardcoded module shape.
        let migration = vec![
            (
                "db/migrations/0001-init.ts",
                "export async function up() {}\nexport async function down() {}\n",
            ),
            ("db/index.ts", "export const z = 1;"),
        ];
        let (_d1, migration_diags) = run_on_project(&migration, "db/migrations/0001-init.ts");
        assert!(
            migration_diags.is_empty(),
            "up+down migration must be exempt via the shared table, got: {migration_diags:?}"
        );

        let yargs = vec![
            (
                "cli/commands/build.ts",
                "export const command = 'build';\nexport const handler = () => {};\n",
            ),
            ("cli/index.ts", "export const z = 1;"),
        ];
        let (_d2, yargs_diags) = run_on_project(&yargs, "cli/commands/build.ts");
        assert!(
            yargs_diags.is_empty(),
            "command+handler yargs module must be exempt via the shared table, got: {yargs_diags:?}"
        );

        let lone_up = vec![
            ("db/migrations/lone.ts", "export const up = () => {};"),
            ("db/index.ts", "export const z = 1;"),
        ];
        let (_d3, lone_diags) = run_on_project(&lone_up, "db/migrations/lone.ts");
        assert_eq!(
            lone_diags.len(),
            1,
            "lone up (gate not satisfied) must still be flagged, got: {lone_diags:?}"
        );
        assert!(lone_diags[0].message.contains("up"));
    }

    #[test]
    fn no_fp_for_nextra_meta_file() {
        // Regression for #2041 — Nextra's per-directory `_meta.tsx` files export
        // a `default` route-metadata object consumed by Nextra's file-system
        // router by filename convention at build time. No TS file imports them,
        // so the `default` export looks dead; dead-export must not flag it.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/content/ko/_meta.tsx",
                "import type { MetaRecord } from 'nextra'\n\
                 export default {\n\
                   index: { type: 'page', display: 'hidden' },\n\
                   docs: { type: 'page', title: '문서' },\n\
                 } satisfies MetaRecord\n",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/content/ko/_meta.tsx");
        assert!(
            diags.is_empty(),
            "Nextra _meta.tsx default export must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_serverless_lambda_handler_in_functions_dir() {
        // Regression for #1771 — sst's Lambda@Edge handler exports a `handler`
        // symbol invoked by the AWS runtime through the deploy config's
        // `handler: "functions/oac-edge-signer/index.handler"` string, never by
        // a static TS import. The file lives in the per-function `functions/`
        // layout, so dead-export must treat the `handler` export as live.
        let files: Vec<(&str, &str)> = vec![
            (
                "platform/functions/oac-edge-signer/index.ts",
                "import { CloudFrontRequestHandler } from \"aws-lambda\";\n\
                 import crypto from \"node:crypto\";\n\
                 export const handler: CloudFrontRequestHandler = async (event) => {\n\
                   const request = event.Records[0].cf.request;\n\
                   return request;\n\
                 };\n",
            ),
            ("platform/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) =
            run_on_project(&files, "platform/functions/oac-edge-signer/index.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("handler")),
            "serverless handler under functions/ must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_serverless_handler_without_aws_lambda_import() {
        // Regression for #1771 — sst's `ssr-warmer` and `nodejs-runtime`
        // function files export `handler` without importing `aws-lambda`. The
        // `functions/` per-function layout is the signal, so dead-export must
        // not flag the `handler` export regardless of the import set.
        let files: Vec<(&str, &str)> = vec![
            (
                "platform/functions/ssr-warmer/index.ts",
                "export const handler = async (event: { time: string }) => {\n\
                   return event;\n\
                 };\n",
            ),
            ("platform/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "platform/functions/ssr-warmer/index.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("handler")),
            "serverless handler under functions/ must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_non_handler_export_in_functions_dir() {
        // Sibling guard for #1771 — the exemption is scoped to the `handler`
        // name. A genuinely dead non-`handler` export under `functions/` with no
        // importer must still be flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "platform/functions/ssr-warmer/index.ts",
                "export const handler = async () => {};\n\
                 export const __DEAD = \"x\";\n",
            ),
            ("platform/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "platform/functions/ssr-warmer/index.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("__DEAD")),
            "non-handler dead export under functions/ must still be flagged, got: {diags:?}"
        );
        assert!(
            diags.iter().all(|d| !d.message.contains("handler")),
            "handler export must remain exempt, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_handler_export_outside_functions_dir() {
        // Sibling guard for #1771 — the exemption is gated on the `functions/`
        // directory. A lone `handler` export NOT under `functions/` is an
        // ordinary export and must still be flagged when unimported.
        let files: Vec<(&str, &str)> = vec![
            ("src/lib/handler.ts", "export const handler = () => {};"),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/lib/handler.ts");
        assert_eq!(
            diags.len(),
            1,
            "handler export outside functions/ must still be flagged, got: {diags:?}"
        );
        assert!(diags[0].message.contains("handler"));
    }

    #[test]
    fn no_fp_for_node_esm_loader_hooks_issue_2301() {
        // Regression for #2301 (angular/angular) — a Node.js ESM customization-
        // hooks module exports `resolve`/`load`/`globalPreload`, which the Node
        // runtime invokes through the `--loader`/`--import` CLI flag, never
        // through a static TS import. The file is a `.mjs` module and the hooks
        // carry the canonical chained-hook signature, so dead-export must treat
        // all three as live.
        let files: Vec<(&str, &str)> = vec![
            (
                "tools/bazel/node_loader/hooks.mjs",
                "export const resolve = async (specifier, context, nextResolve) => {\n\
                   return nextResolve(specifier, context);\n\
                 };\n\
                 export const load = async (url, context, nextLoad) => {\n\
                   return nextLoad(url, context);\n\
                 };\n\
                 export const globalPreload = () => '';\n",
            ),
            ("tools/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "tools/bazel/node_loader/hooks.mjs");
        assert!(
            diags.is_empty(),
            "Node ESM loader hooks (resolve/load/globalPreload) must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_ordinary_resolve_export_in_ts_file() {
        // Sibling guard for #2301 — `resolve`/`load` are extremely common export
        // names. An ordinary `export const resolve = (x) => x` in a `.ts` file
        // (no `.mjs`/`.mts` convention, wrong signature) that nothing imports
        // must still be flagged: exempting it would be a false negative.
        let files: Vec<(&str, &str)> = vec![
            ("src/utils.ts", "export const resolve = (x: number) => x;\n"),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/utils.ts");
        assert_eq!(
            diags.len(),
            1,
            "ordinary resolve export in a .ts file must still be flagged, got: {diags:?}"
        );
        assert!(diags[0].message.contains("resolve"));
    }

    #[test]
    fn still_flags_resolve_in_mjs_without_chained_hook_signature() {
        // Sibling guard for #2301 — the ESM extension alone is not enough; the
        // chained-hook shape is the second, stronger half of the signal. A
        // `resolve` export in a `.mjs` module whose last parameter is NOT the
        // `nextResolve` continuation is an ordinary export and must still fire.
        let files: Vec<(&str, &str)> = vec![
            ("src/paths.mjs", "export const resolve = (a, b) => a + b;\n"),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/paths.mjs");
        assert_eq!(
            diags.len(),
            1,
            "resolve in .mjs without the chained-hook signature must still be flagged, got: {diags:?}"
        );
        assert!(diags[0].message.contains("resolve"));
    }

    #[test]
    fn still_flags_lone_global_preload_in_mjs() {
        // Sibling guard for #2301 — `globalPreload` has too generic a signature
        // to identify the convention on its own. Without a shape-valid sibling
        // `resolve`/`load` in the same module, a lone `globalPreload` export
        // stays subject to the rule.
        let files: Vec<(&str, &str)> = vec![
            ("src/preload.mjs", "export const globalPreload = () => '';\n"),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/preload.mjs");
        assert_eq!(
            diags.len(),
            1,
            "lone globalPreload without a sibling hook must still be flagged, got: {diags:?}"
        );
        assert!(diags[0].message.contains("globalPreload"));
    }

    #[test]
    fn still_flags_meta_file_without_leading_underscore() {
        // Sibling guard for #2041 — an ordinary `meta.ts` (no leading
        // underscore) is not a Nextra convention file and must still be flagged
        // when its export has no importer.
        let files: Vec<(&str, &str)> = vec![
            ("src/content/meta.ts", "export default { title: 'x' };"),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/content/meta.ts");
        assert_eq!(
            diags.len(),
            1,
            "ordinary meta.ts (no underscore) must still be flagged, got: {diags:?}"
        );
        assert!(diags[0].message.contains("default"));
    }

    #[test]
    fn backend_silenced_without_entrypoints_configured() {
        // Without entrypoints configured, backend files under /api/ dirs are
        // still silenced (backward-compat).
        let pkg = r#"{ "dependencies": { "elysia": "1.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/api/features/x/schema.ts",
                "export const __DEAD = \"x\";",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) =
            run_on_project_with_pkg(Some(pkg), &files, "src/api/features/x/schema.ts");
        assert!(
            diags.is_empty(),
            "without entrypoints, backend /api/ files must remain silenced (backward-compat), got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_docusaurus_site_alias_importer() {
        // Regression for #2014 — Docusaurus maps the `@site/` alias to the site
        // root via webpack, so `import HeroLerna from "@site/src/components/..."`
        // never resolves in the import index. The imported component's `default`
        // export looks dead even though the page consumes it; dead-export must
        // not flag it.
        let files: Vec<(&str, &str)> = vec![
            (
                "website/src/components/hero-lerna.tsx",
                "export default function HeroLerna() { return null; }",
            ),
            (
                "website/src/pages/index.tsx",
                "import HeroLerna from \"@site/src/components/hero-lerna\";\n\
                 export default function Home() { return HeroLerna; }",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "website/src/components/hero-lerna.tsx");
        assert!(
            diags.is_empty(),
            "component imported via @site/ alias must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_export_with_no_site_alias_importer() {
        // Sibling guard for #2014 — a genuinely dead component (no importer at
        // all, including no matching `@site/` alias) must still be flagged. A
        // non-matching `@site/` import elsewhere must not suppress it.
        let files: Vec<(&str, &str)> = vec![
            (
                "website/src/components/orphan.tsx",
                "export default function Orphan() { return null; }",
            ),
            (
                "website/src/pages/index.tsx",
                "import HeroLerna from \"@site/src/components/hero-lerna\";\n\
                 export default function Home() { return HeroLerna; }",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "website/src/components/orphan.tsx");
        assert_eq!(
            diags.len(),
            1,
            "dead component with no matching @site/ importer must still be flagged, got: {diags:?}"
        );
        assert!(diags[0].message.contains("default"));
    }

    /// Run dead-export after also writing non-source sidecar files (e.g.
    /// `ng-package.json`) that must NOT be added to the import index.
    fn run_on_project_with_extra(
        extra_files: &[(&str, &str)],
        files: &[(&str, &str)],
        target_rel: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        for (rel, content) in extra_files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
        }
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
        let project = ProjectCtx::load(&refs, &Config::default());

        let target_path: PathBuf = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &Config::default(),
            project: &project,
            file: &file_ctx,
            lang: crate::files::Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        (dir, diags)
    }

    #[test]
    fn no_fp_for_ng_packagr_entry_file() {
        // Regression for #1840 — an ng-packagr Angular library declares its
        // public-API entry in `ng-package.json` (`lib.entryFile`), not in
        // `package.json` `main`/`exports` (ng-packagr emits those to the build
        // output). The entry barrel `public_api.ts` re-exports the module, but
        // no source file imports it, so both the entry file's re-export and the
        // re-exported `IonicServerModule` look dead. Neither must be flagged.
        let extra = vec![(
            "packages/angular-server/ng-package.json",
            "{ \"lib\": { \"entryFile\": \"src/public_api.ts\" } }",
        )];
        let files: Vec<(&str, &str)> = vec![
            (
                "packages/angular-server/src/public_api.ts",
                "export { IonicServerModule } from './ionic-server-module';",
            ),
            (
                "packages/angular-server/src/ionic-server-module.ts",
                "export class IonicServerModule {}",
            ),
        ];
        let (_dir, entry_diags) = run_on_project_with_extra(
            &extra,
            &files,
            "packages/angular-server/src/public_api.ts",
        );
        assert!(
            entry_diags.is_empty(),
            "ng-packagr entry file must not be flagged, got: {entry_diags:?}"
        );

        let (_dir2, module_diags) = run_on_project_with_extra(
            &extra,
            &files,
            "packages/angular-server/src/ionic-server-module.ts",
        );
        assert!(
            module_diags.is_empty(),
            "symbol re-exported by the ng-packagr entry barrel must not be flagged, got: {module_diags:?}"
        );
    }

    #[test]
    fn still_flags_dead_export_in_ng_packagr_library() {
        // Sibling guard for #1840 — a symbol that is NOT re-exported by the
        // ng-packagr public-API entry barrel and has no importer is genuinely
        // dead and must still be flagged. Presence of an `ng-package.json` must
        // not blanket-exempt the whole package.
        let extra = vec![(
            "packages/angular-server/ng-package.json",
            "{ \"lib\": { \"entryFile\": \"src/public_api.ts\" } }",
        )];
        let files: Vec<(&str, &str)> = vec![
            (
                "packages/angular-server/src/public_api.ts",
                "export { IonicServerModule } from './ionic-server-module';",
            ),
            (
                "packages/angular-server/src/ionic-server-module.ts",
                "export class IonicServerModule {}",
            ),
            (
                "packages/angular-server/src/internal-helper.ts",
                "export function unusedHelper() {}",
            ),
        ];
        let (_dir, diags) = run_on_project_with_extra(
            &extra,
            &files,
            "packages/angular-server/src/internal-helper.ts",
        );
        assert!(
            diags.iter().any(|d| d.message.contains("unusedHelper")),
            "private symbol not on the public API must still be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn handles_ng_packagr_secondary_entry_point() {
        // ng-packagr secondary entry points live in nested `ng-package.json`
        // files (and their JSONC often carries a trailing comma). The nearest
        // `ng-package.json` to a file in `standalone/` is the nested one, so its
        // `lib.entryFile` is the entry for that subtree.
        let extra = vec![(
            "packages/angular/standalone/ng-package.json",
            "{\n  \"lib\": {\n    \"entryFile\": \"src/index.ts\"\n  },\n}\n",
        )];
        let files: Vec<(&str, &str)> = vec![
            (
                "packages/angular/standalone/src/index.ts",
                "export { StandaloneThing } from './standalone-thing';",
            ),
            (
                "packages/angular/standalone/src/standalone-thing.ts",
                "export class StandaloneThing {}",
            ),
        ];
        let (_dir, diags) = run_on_project_with_extra(
            &extra,
            &files,
            "packages/angular/standalone/src/standalone-thing.ts",
        );
        assert!(
            diags.is_empty(),
            "secondary ng-packagr entry barrel must seed reachability, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_symbol_consumed_through_star_reexport_barrel() {
        // Regression for #1881 — a utility module's export is re-exported via
        // `export * from './misc/getTreeDiff'` in a barrel and consumed by
        // importing the symbol from the barrel. No file imports the source
        // module directly, so without following `export *` chains the symbol
        // looks dead even though it is used transitively.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/utils/misc/getTreeDiff.ts",
                "export function getTreeDiff(prev: unknown, next: unknown): unknown {\n  return [prev, next];\n}\n",
            ),
            (
                "src/utils/index.ts",
                "export * from './misc/getTreeDiff';\n",
            ),
            (
                "src/system/pointer/pointer.ts",
                "import { getTreeDiff } from '../../utils';\ngetTreeDiff(null, null);\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/utils/misc/getTreeDiff.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("getTreeDiff")),
            "symbol re-exported via `export *` barrel and imported from it must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_symbol_in_star_reexported_module_when_barrel_unused() {
        // Sibling guard — a module re-exported via `export *` from a barrel
        // that nobody imports the symbol from is genuinely dead.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/utils/misc/getTreeDiff.ts",
                "export function getTreeDiff(prev: unknown, next: unknown): unknown {\n  return [prev, next];\n}\n",
            ),
            (
                "src/utils/index.ts",
                "export * from './misc/getTreeDiff';\n",
            ),
            ("src/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/utils/misc/getTreeDiff.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("getTreeDiff")),
            "symbol nobody imports through the barrel is still dead, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_bin_package_internal_export() {
        // Regression for #1141 — azure-sdk-for-js's `@azure/dev-tool` is a
        // CLI-tool workspace package (declares `bin`, no `main`/`exports`/
        // `module`). Its `src/**` is the tool's implementation; sibling packages
        // consume it by invoking the `dev-tool` binary, never by ES-importing its
        // util modules. Internal helpers like `isMigrationSuspended` are wired up
        // by the command framework and referenced only inside function bodies, so
        // they have no static importer and look dead. A package that publishes a
        // `bin` entry is consumed as a binary — dead-export must not flag its
        // source exports.
        let extra = vec![(
            "common/tools/dev-tool/package.json",
            r#"{ "name": "@azure/dev-tool", "bin": { "dev-tool": "launch.ts" } }"#,
        )];
        let files: Vec<(&str, &str)> = vec![
            (
                "common/tools/dev-tool/src/util/migrations.ts",
                "export async function isMigrationSuspended(): Promise<boolean> {\n  return false;\n}\n\
                 export async function run(): Promise<void> {\n  if (await isMigrationSuspended()) return;\n}\n",
            ),
            ("sdk/storage/storage-blob/src/index.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project_with_extra(
            &extra,
            &files,
            "common/tools/dev-tool/src/util/migrations.ts",
        );
        assert!(
            diags.is_empty(),
            "bin-package internal exports must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_dead_export_in_non_bin_package() {
        // Sibling guard for #1141 — a regular package (no `bin`, no
        // `main`/`exports`/`module`) with a genuinely unused export must still be
        // flagged. The `bin` exemption must not blanket-silence ordinary packages.
        let extra = vec![(
            "packages/lib/package.json",
            r#"{ "name": "@scope/lib" }"#,
        )];
        let files: Vec<(&str, &str)> = vec![
            ("packages/lib/src/dead.ts", "export function unused() {}"),
            ("packages/lib/src/index.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project_with_extra(
            &extra,
            &files,
            "packages/lib/src/dead.ts",
        );
        assert!(
            diags.iter().any(|d| d.message.contains("unused")),
            "genuinely dead export in a non-bin package must still be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_vitest_workspace_default_export() {
        // Regression for #1141 — `vitest.workspace.ts` exports `default` as the
        // Vitest workspace configuration. Vitest loads this file by convention
        // (filename), never through a TS `import`, so the `default` export has no
        // static importer and looks dead. dead-export must not flag it.
        let files: Vec<(&str, &str)> = vec![
            (
                "vitest.workspace.ts",
                "export default ['packages/*'];\n",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "vitest.workspace.ts");
        assert!(
            diags.is_empty(),
            "vitest.workspace.ts default export must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_export_in_declare_module_augmentation_issue_1731() {
        // Regression for #1731 (pinia) — an `export interface` inside a
        // `declare module '...'` block is a TypeScript module augmentation: the
        // compiler merges it into the augmented module's types, so it is never
        // imported by name. dead-export must not flag it.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/augment.ts",
                "import { RouteLocationNormalizedLoaded } from 'vue-router';\n\
                 declare module 'pinia' {\n\
                   export interface PiniaCustomProperties {\n\
                     get route(): RouteLocationNormalizedLoaded\n\
                   }\n\
                 }\n",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/augment.ts");
        assert!(
            diags.is_empty(),
            "export inside declare module augmentation must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn flags_top_level_export_alongside_declare_module_augmentation() {
        // Negative-space guard for #1731 — the augmentation exemption is scoped
        // to exports nested in a `declare module` block. A genuinely unused
        // top-level export in the same file is still flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/augment.ts",
                "declare module 'pinia' {\n\
                   export interface PiniaCustomProperties {\n\
                     count: number\n\
                   }\n\
                 }\n\
                 export const unusedHelper = 1;\n",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/augment.ts");
        assert_eq!(
            diags.len(),
            1,
            "only the top-level unusedHelper is dead, got: {diags:?}"
        );
        assert!(
            diags[0].message.contains("unusedHelper"),
            "message should name the top-level dead export, got: {}",
            diags[0].message
        );
    }

    // Regression #1800 (wagmi): a monorepo whose root `package.json` declares no
    // framework, with Next.js listed only in a nested sub-package
    // (`playgrounds/next/package.json`). The App Router file's `metadata` and
    // `default` exports are consumed by Next.js's file-system router, never via a
    // static import — they must not be flagged even though the framework is
    // invisible to root-anchored detection.
    #[test]
    fn no_fp_for_nextjs_app_router_in_nested_package_issue_1800() {
        let extra = vec![
            ("package.json", r#"{"name":"workspace","private":true}"#),
            (
                "playgrounds/next/package.json",
                r#"{"name":"@wagmi/next-playground","dependencies":{"next":"^15.0.0"}}"#,
            ),
        ];
        let files: Vec<(&str, &str)> = vec![
            (
                "playgrounds/next/src/app/layout.tsx",
                "export const metadata = { title: 'Create Wagmi' };\n\
                 export default function RootLayout() { return null; }\n",
            ),
            // A second file gives the index more than one entry so the rule runs.
            ("packages/lib/src/index.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) =
            run_on_project_with_extra(&extra, &files, "playgrounds/next/src/app/layout.tsx");
        assert!(
            diags.is_empty(),
            "Next.js App Router exports under a nested sub-package must not be flagged, got: {diags:?}"
        );
    }

    // Negative-space guard for #1800: when NO framework dep exists anywhere
    // (neither root nor any sub-package), an unimported export under an `app/`
    // directory is genuinely dead and must still be flagged. Confirms the
    // nested-detection fallback does not blanket-exempt every `app/` file.
    #[test]
    fn flags_dead_export_in_app_dir_without_framework_issue_1800() {
        let extra = vec![
            ("package.json", r#"{"name":"workspace","private":true}"#),
            ("packages/lib/package.json", r#"{"name":"@scope/lib"}"#),
        ];
        let files: Vec<(&str, &str)> = vec![
            ("packages/lib/src/app/helper.ts", "export function unusedHelper() {}\n"),
            ("packages/lib/src/app/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) =
            run_on_project_with_extra(&extra, &files, "packages/lib/src/app/helper.ts");
        assert_eq!(
            diags.len(),
            1,
            "a dead export under app/ with no framework anywhere must still fire, got: {diags:?}"
        );
        assert!(
            diags[0].message.contains("unusedHelper"),
            "message should name the dead export, got: {}",
            diags[0].message
        );
    }

    // --- Issue #1833: React Router v7 `root.tsx` / `routes.ts` conventions ---

    #[test]
    fn ignores_react_router_v7_root_module_exports_issue_1833() {
        // Regression for #1833 (pmndrs/react-spring) — React Router v7's app root
        // module (`app/root.tsx`) exports `Layout`, `links`, `meta`, and a default
        // component that its server/client render pipeline consumes by exact name,
        // never through a static import. The project depends on `react-router`
        // (v7, the framework formerly Remix), not `@remix-run/*`.
        let pkg = r#"{ "dependencies": { "react-router": "7.0.0" }, "devDependencies": { "@react-router/dev": "7.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "app/root.tsx",
                "export const meta = () => [{ title: \"x\" }];\n\
                 export const links = () => [];\n\
                 export function Layout({ children }) { return children; }\n\
                 export default function App() { return null; }\n",
            ),
            ("app/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "app/root.tsx");
        assert!(
            diags.is_empty(),
            "React Router v7 root.tsx convention exports are framework-consumed: {diags:?}"
        );
    }

    #[test]
    fn ignores_react_router_v7_routes_config_default_export_issue_1833() {
        // Regression for #1833 — the route-config entry (`app/routes.ts`) default
        // export is consumed by `@react-router/dev`, never statically imported.
        let pkg = r#"{ "dependencies": { "react-router": "7.0.0" }, "devDependencies": { "@react-router/dev": "7.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "app/routes.ts",
                "export default [{ path: \"/\", file: \"home.tsx\" }];\n",
            ),
            ("app/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "app/routes.ts");
        assert!(
            diags.is_empty(),
            "React Router v7 routes.ts default export is framework-consumed: {diags:?}"
        );
    }

    #[test]
    fn still_flags_ordinary_export_in_react_router_root_module_issue_1833() {
        // Negative-space guard for #1833 — the exemption covers only React Router's
        // reserved root names. An ordinary `helper` export in `root.tsx`, with no
        // importer, is genuinely dead and must still be flagged.
        let pkg = r#"{ "dependencies": { "react-router": "7.0.0" }, "devDependencies": { "@react-router/dev": "7.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "app/root.tsx",
                "export function Layout({ children }) { return children; }\n\
                 export const helper = () => 1;\n",
            ),
            ("app/util.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "app/root.tsx");
        assert_eq!(
            diags.len(),
            1,
            "an ordinary unused export in root.tsx must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("helper"));
    }

    #[test]
    fn still_flags_layout_export_in_non_root_module_issue_1833() {
        // Negative-space guard for #1833 — `Layout`/`links`/`meta` are common
        // generic names. A `Layout` export from an ordinary module (not the app
        // root or a route file), with no importer, is genuinely dead and must
        // still fire: the React Router exemption is scoped to convention files.
        let pkg = r#"{ "dependencies": { "react-router": "7.0.0" }, "devDependencies": { "@react-router/dev": "7.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "app/components/Shell.tsx",
                "export function Layout({ children }) { return children; }\n",
            ),
            ("app/components/other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) =
            run_on_project_with_pkg(Some(pkg), &files, "app/components/Shell.tsx");
        assert_eq!(
            diags.len(),
            1,
            "a `Layout` export in a non-convention module must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("Layout"));
    }

    #[test]
    fn ignores_docusaurus_theme_swizzle_default_export_issue_1558() {
        // Regression for #1558 — a Docusaurus theme override under `src/theme/`
        // is discovered by the theme system through its path and webpack theme
        // aliases, never a static import, so its `default` export looks dead.
        // Entrypoints are configured to disable the framework-dir bail-out, so
        // only the per-export magic-export path can protect it.
        let pkg = r#"{ "dependencies": { "@docusaurus/core": "3.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/theme/MDXComponents/index.tsx",
                "export default function MDXComponents() { return null; }\n",
            ),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg_and_entrypoints(
            pkg,
            vec!["src/server.ts".to_string()],
            &files,
            "src/theme/MDXComponents/index.tsx",
        );
        assert!(
            diags.is_empty(),
            "Docusaurus theme swizzle default export must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn ignores_docusaurus_plugin_default_export_issue_1558() {
        // Regression for #1558 — a Docusaurus local plugin (`plugins/*/index.ts`)
        // is loaded by the path string in `docusaurus.config`, calling its
        // `default` export, never a static import.
        let pkg = r#"{ "dependencies": { "@docusaurus/core": "3.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "plugins/recent-blog-posts/index.ts",
                "export default function recentBlogPostsPlugin() { return {}; }\n",
            ),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) =
            run_on_project_with_pkg(Some(pkg), &files, "plugins/recent-blog-posts/index.ts");
        assert!(
            diags.is_empty(),
            "Docusaurus plugin default export must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn ignores_knip_config_default_export_issue_1558() {
        // Regression for #1558 — `knip.ts` exports its config as `export default`;
        // the Knip tool reads the file by name and never `import`s it.
        let files: Vec<(&str, &str)> = vec![
            ("knip.ts", "export default { entry: ['src/index.ts'] };\n"),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "knip.ts");
        assert!(
            diags.is_empty(),
            "knip.ts config default export must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_theme_default_export_in_non_docusaurus_project_issue_1558() {
        // Negative-space guard for #1558 — the theme exemption is gated on
        // Docusaurus detection. A `src/theme/` default export in a project with
        // no Docusaurus dependency is genuinely dead and must still be flagged.
        let pkg = r#"{ "dependencies": { "react": "18.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/theme/Foo.tsx",
                "export default function Foo() { return null; }\n",
            ),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/theme/Foo.tsx");
        assert_eq!(
            diags.len(),
            1,
            "a theme default export in a non-Docusaurus project must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("default"));
    }

    #[test]
    fn still_flags_named_export_in_docusaurus_theme_file_issue_1558() {
        // Negative-space guard for #1558 — only the `default` export of a theme
        // swizzle is magic. A plain named export in the same file, with no
        // importer, is genuinely dead and must still be flagged. Entrypoints are
        // configured to disable the framework-dir bail-out so the per-export
        // check is what decides.
        let pkg = r#"{ "dependencies": { "@docusaurus/core": "3.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/theme/MDXComponents/index.tsx",
                "export default function MDXComponents() { return null; }\n\
                 export const unusedHelper = () => 1;\n",
            ),
            ("src/util.ts", "export const helper = () => 1;\nhelper;\n"),
        ];
        let (_dir, diags) = run_on_project_with_pkg_and_entrypoints(
            pkg,
            vec!["src/server.ts".to_string()],
            &files,
            "src/theme/MDXComponents/index.tsx",
        );
        assert_eq!(
            diags.len(),
            1,
            "a named export in a Docusaurus theme file must still be flagged: {diags:?}"
        );
        assert!(diags[0].message.contains("unusedHelper"));
    }

    // Regression for #2201 (TanStack/virtual) — a demo app's entry file under a
    // top-level `examples/` directory exports its mounted instance by the Vite
    // `export default app` convention; nothing imports it, but it is a runnable
    // demo entry, not dead code. Nested `examples/.../src/main.ts` is missed by
    // the root-only `is_entry_point` and the example's non-library
    // `package.json`, so the sample-dir guard must carry it.
    #[test]
    fn no_fp_for_export_in_examples_dir_main_issue_2201() {
        let files: Vec<(&str, &str)> = vec![
            (
                "examples/svelte/infinite-scroll/src/main.ts",
                "import App from './App.svelte'\n\
                 const app = new App({ target: document.getElementById('app')! })\n\
                 export default app\n",
            ),
            ("src/index.ts", "export const used = 1;\nused;\n"),
        ];
        let (_dir, diags) =
            run_on_project(&files, "examples/svelte/infinite-scroll/src/main.ts");
        assert!(
            diags.is_empty(),
            "an export in a top-level examples/ demo entry must not be flagged: {diags:?}"
        );
    }

    // Negative-space guard for #2201 — the sample-dir exemption is scoped to
    // demonstration directories. An unimported export in an ordinary source
    // file (not under examples/demo/sample) is genuinely dead and still fires.
    #[test]
    fn still_flags_unimported_export_in_normal_source_issue_2201() {
        let files: Vec<(&str, &str)> = vec![
            ("src/lib/foo.ts", "export const orphan = 1;\n"),
            ("src/index.ts", "export const used = 1;\nused;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/lib/foo.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("orphan")),
            "an unimported export in a normal source file must still be flagged: {diags:?}"
        );
    }
}
