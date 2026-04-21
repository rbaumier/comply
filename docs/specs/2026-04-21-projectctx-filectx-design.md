# ProjectCtx + FileCtx — Design (Chantier #1)

**Date:** 2026-04-21
**Scope:** Infrastructure to make project-level and file-level context available to every rule via `CheckCtx`.
**Status:** Draft for review.

---

## 1. Problem

Rules currently receive `CheckCtx { path, source, config }`. Anything beyond "this file's bytes" they re-derive from scratch:

- 4 rules re-parse `package.json` on every file lint (`no_implicit_deps`, `prefer_global_this`, `package_json_unique_deps`, `package_json_sorted_deps`).
- Rules that need to know "is this a Next.js project?" or "are we in `app/` vs `pages/`?" today guess from the file path with ad-hoc substring checks.
- RSC-aware rules (chantier #2) have nowhere to put `"use client"` / `"use server"` state — it would be re-scanned per rule.
- Tailwind-aware rules (magic spacing, arbitrary colors) need the theme. Today they ship a hardcoded fallback.
- Drizzle, tsconfig paths, workspace roots — same story.

This blocks ~17 rules classified "Difficile" in `RULES_TO_ADD.md`, and forces every future project-aware rule to reinvent IO.

## 2. Consumer consequence

**Rule authors** get two new context objects available on every check: one that's loaded once per run (project), one per file (file). No more "read package.json inside the rule". No more path-substring guessing. Tests get `for_test()` helpers so rules stay unit-testable.

**End users** see no behavior change in existing rules, but future project-aware rules (Tailwind, RSC, Drizzle) start working without perf regression because manifests are parsed once, not once-per-file-per-rule.

## 3. How

- Add `ProjectCtx` (Arc-shared, loaded once in `lint_files`) and `FileCtx` (per-file, built in `dispatch_backends`).
- Extend `CheckCtx` to carry both by reference.
- Migrate the 4 existing `package.json` readers in the same PR as dogfood.
- Loading strategy: eager for cheap manifests (`package.json`, `tsconfig.json`), lazy via `OnceLock` for expensive ones (Tailwind theme, Drizzle config).

---

## 4. ProjectCtx

Loaded **once** at the start of `lint_files`. Wrapped in `Arc`, borrowed by every `CheckCtx`.

```rust
pub struct ProjectCtx {
    // Eager — cheap, always read at startup
    pub project_root: PathBuf,
    pub workspace_roots: Vec<PathBuf>,     // monorepo packages
    pub package_json: Option<PackageJson>, // root manifest
    pub tsconfig: Option<Tsconfig>,        // root tsconfig.json
    pub framework: Framework,              // deduced from deps

    // Lazy — only paid if a rule asks
    tailwind_theme: OnceLock<Option<TailwindTheme>>,
    drizzle_config: OnceLock<Option<DrizzleConfig>>,
}

pub struct PackageJson {
    pub name: Option<String>,
    pub version: Option<String>,
    pub module_type: ModuleType,                   // "module" / "commonjs"
    pub dependencies: BTreeMap<String, String>,
    pub dev_dependencies: BTreeMap<String, String>,
    pub peer_dependencies: BTreeMap<String, String>,
    pub workspaces: Vec<String>,                   // glob patterns
}

pub struct Tsconfig {
    pub paths: BTreeMap<String, Vec<String>>,      // path aliases
    pub base_url: Option<PathBuf>,
    pub module: Option<String>,                    // "esnext", "commonjs"…
    pub strict: bool,
    pub jsx: Option<String>,
}

pub enum Framework {
    NextJs,
    TanStackStart,
    Vue,
    Nuxt,
    Remix,
    SvelteKit,
    Plain,
}

pub struct TailwindTheme {
    pub colors: BTreeMap<String, String>,   // v4 @theme or static v3 object
    pub spacing: BTreeMap<String, String>,
    pub source: TailwindSource,              // CssV4 | TsV3Static | None
}

pub struct DrizzleConfig {
    pub driver: Option<String>,              // "pg" | "mysql" | "sqlite" | ...
    pub schema_paths: Vec<PathBuf>,
}
```

### Loading rules

| Field | Timing | Cost if absent | Malformed |
|---|---|---|---|
| `package_json` | eager | `None` | `None` + one stderr warning |
| `tsconfig` | eager | `None` | `None` + one stderr warning |
| `framework` | eager (derived) | `Plain` | `Plain` |
| `workspace_roots` | eager | `vec![]` | `vec![]` |
| `tailwind_theme` | lazy via `OnceLock` | `None` | `None`, cached |
| `drizzle_config` | lazy via `OnceLock` | `None` | `None`, cached |

Eager fields are read synchronously before spawning the rayon loop. They're small files (<100KB typical), IO-bound, and almost always present in the projects comply targets.

Lazy fields use `OnceLock<Option<T>>` — the first rule that calls `ctx.project.tailwind_theme()` triggers loading; subsequent calls hit the cached value. Since rayon parallelises per-file, the first hit races, but `OnceLock` serialises the write and all subsequent readers see the same result.

**Malformed manifests** cache `None` (never retry within the run) and emit exactly one stderr warning per field. Matches the current behaviour of `no_implicit_deps` and `prefer_global_this`, which silently degrade today.

### Monorepo accessors

A single root `package_json` isn't enough for monorepos — today `no_implicit_deps` walks up from the source file to find the *nearest* manifest. `ProjectCtx` preserves that by exposing two lookups:

```rust
impl ProjectCtx {
    /// Walk up from `path` to the nearest package.json (cached).
    pub fn nearest_package_json(&self, path: &Path) -> Option<&PackageJson>;
    /// Walk up from `path` to the nearest tsconfig.json (cached).
    pub fn nearest_tsconfig(&self, path: &Path) -> Option<&Tsconfig>;
}
```

Results are memoised in a `DashMap<PathBuf, Arc<PackageJson>>` keyed by *manifest path* (not source path), so two sibling files pointing at the same manifest share one parse.

### Tailwind loading — hybrid static

- v4 CSS-first: parse `@theme { --color-foo: …; }` blocks out of `.css` files. Static scan, no JS/CSS runtime.
- v3 TS config: parse `tailwind.config.{ts,js}` with tree-sitter, extract the top-level `theme.extend.colors` / `theme.colors` object-literal keys. **Static-only** — no `require()` evaluation, no dynamic config.
- If both exist, v4 wins.
- If config uses dynamic expressions (imports, spread from another module, function calls), we return `None` for that field rather than a partial result. Rules then skip silently — better than a wrong theme.

## 5. FileCtx

Built **per-file** in `dispatch_backends`. Cheap — all fields are either path-based or a single linear scan over the source prefix.

```rust
pub struct FileCtx {
    pub language: Language,             // Ts | Tsx | Js | Rust | Vue
    pub directives: FileDirectives,
    pub rsc_context: RscContext,
    pub path_segments: PathSegments,
}

pub struct FileDirectives {
    pub use_client: bool,
    pub use_server: bool,
}

pub enum RscContext {
    ServerComponent, // Next.js App Router default (no "use client")
    ClientComponent, // "use client" directive
    ServerFunction,  // "use server" directive at top
    Unknown,         // not in an RSC-aware framework or ambiguous
}

pub struct PathSegments {
    pub in_app_router: bool,    // .../app/...
    pub in_pages_router: bool,  // .../pages/...
    pub in_test_dir: bool,      // /tests/, /__tests__/, *.test.*, *.spec.*
    pub in_node_modules: bool,  // should always be false — engine filters
    pub in_storybook: bool,     // *.stories.*
}
```

`directives` comes from a single scan over the first ~20 non-whitespace/non-comment tokens of the source. `rsc_context` combines `directives` + `framework` + `path_segments` (e.g. in `app/` under Next.js without `"use client"` → `ServerComponent`). `path_segments` is pure path manipulation, no IO.

## 6. New CheckCtx

```rust
pub struct CheckCtx<'a> {
    pub path: &'a Path,
    pub source: &'a str,
    pub config: &'a Config,
    pub project: &'a ProjectCtx,  // Arc'd upstream, borrowed here
    pub file: &'a FileCtx,
}

impl<'a> CheckCtx<'a> {
    #[cfg(test)]
    pub fn for_test(path: &'a Path, source: &'a str) -> Self;
    #[cfg(test)]
    pub fn for_test_with_project(
        path: &'a Path, source: &'a str, project: &'a ProjectCtx,
    ) -> Self;
}
```

**Preserving the 2-arg `for_test` surface is load-bearing.** 218 call sites across 199 test files already use `CheckCtx::for_test(path, source)` — the signature must not change, or the migration touches every rule test.

To keep the signature 2-arg while `CheckCtx` holds `&'a ProjectCtx` and `&'a FileCtx`, we mirror the existing `default_static_config()` pattern:

```rust
// src/rules/backend.rs (alongside default_static_config)
pub(crate) fn default_static_project_ctx() -> &'static ProjectCtx {
    static DEFAULT: OnceLock<ProjectCtx> = OnceLock::new();
    DEFAULT.get_or_init(ProjectCtx::empty)
}

pub(crate) fn default_static_file_ctx() -> &'static FileCtx {
    static DEFAULT: OnceLock<FileCtx> = OnceLock::new();
    DEFAULT.get_or_init(FileCtx::empty)
}
```

`ProjectCtx::empty()` returns a zero-value instance (`package_json: None`, `framework: Plain`, empty maps, initialised `OnceLock`s cached as `None`). Infallible, no IO.

`FileCtx::empty()` returns all-false directives, `RscContext::Unknown`, `PathSegments::default()`. The test fixture `run_ts/run_tsx/run_rust` in `test_helpers.rs` builds a real `FileCtx` from the path + source so RSC-aware rule tests get correct context without extra ceremony.

Rules that don't read `ctx.project` or `ctx.file` pay nothing — the static defaults are initialised once per process.

## 7. Engine wiring

The engine has **two entry points** that both need `ProjectCtx` + `FileCtx`:

- `lint_files(files: &[&SourceFile], config: &Config) -> Result<Vec<Diagnostic>>` — batch / CLI path. Reads each file from disk in `lint_one_file`, dispatches via `dispatch_backends`.
- `lint_in_memory(path, language, source, config) -> Vec<Diagnostic>` — LSP path. In-memory source, single file, no disk read.

Both funnel through `dispatch_backends`, so that's where `FileCtx` gets built. `ProjectCtx` is built once per entry-point invocation and threaded through.

```rust
// Batch path
pub fn lint_files(files: &[&SourceFile], config: &Config) -> Result<Vec<Diagnostic>> {
    let project = Arc::new(ProjectCtx::load(files, config));
    let rule_defs = rules::all_rule_defs();
    let mut diagnostics: Vec<Diagnostic> = files
        .par_iter()
        .map_init(Parser::new, |parser, file| {
            match lint_one_file(file, &rule_defs, parser, config, &project) {
                Ok(d) => d,
                Err(e) => { eprintln!("comply: skipping {}: {e:#}", file.path.display()); Vec::new() }
            }
        })
        .flatten()
        .collect();
    diagnostics.retain(|d| !is_self_reference(d));
    Ok(diagnostics)
}

fn lint_one_file(
    file: &SourceFile,
    rule_defs: &[RuleDef],
    parser: &mut Parser,
    config: &Config,
    project: &ProjectCtx,
) -> Result<Vec<Diagnostic>> {
    let source = fs::read_to_string(&file.path)
        .with_context(|| format!("failed to read {}", file.path.display()))?;
    let applicable = collect_applicable(rule_defs, file.language);
    if applicable.is_empty() { return Ok(vec![]); }
    Ok(dispatch_backends(file, &source, &applicable, parser, config, project))
}

// LSP path — accepts an optional shared ProjectCtx
pub fn lint_in_memory(
    path: &Path,
    language: Language,
    source: &str,
    config: &Config,
    project: Option<&ProjectCtx>, // None → static empty default
) -> Vec<Diagnostic> {
    let rule_defs = rules::all_rule_defs();
    let applicable = collect_applicable(&rule_defs, language);
    if applicable.is_empty() { return Vec::new(); }
    let file = SourceFile { path: path.to_path_buf(), language };
    let mut parser = Parser::new();
    let project = project.unwrap_or_else(default_static_project_ctx);
    dispatch_backends(&file, source, &applicable, &mut parser, config, project)
}

fn dispatch_backends(
    file: &SourceFile, source: &str,
    applicable: &[(&RuleMeta, &Backend)],
    parser: &mut Parser,
    config: &Config,
    project: &ProjectCtx,
) -> Vec<Diagnostic> {
    // ... existing cull + parse logic unchanged ...
    let file_ctx = FileCtx::build(&file.path, source, file.language, project);
    let ctx = CheckCtx {
        path: &file.path, source, config,
        project, file: &file_ctx,
    };
    // ... existing dispatch loop unchanged ...
}
```

**Project-root detection.** `ProjectCtx::load(files, config)` picks the root in this order:
1. Common ancestor of `files`, walk up to nearest `package.json`.
2. If none found, walk up to nearest `.git`.
3. If still none, use the common ancestor itself. Framework stays `Plain`.

For `lint_in_memory` without an explicit `ProjectCtx`, callers pass `None` and get the static empty default — LSP servers that want real project context build one at workspace-open time and thread it through.

## 8. Migration — same PR

Rules that read project manifests today migrate in the same PR as the infra, proving the API:

| Rule | Reads today | After |
|---|---|---|
| `no_implicit_deps` | `package.json` AND `tsconfig.json`, both via walk-up from source file | `ctx.project.nearest_package_json(ctx.path)` + `ctx.project.nearest_tsconfig(ctx.path)` |
| `prefer_global_this` | `package.json` to check `"type": "module"` | `ctx.project.nearest_package_json(ctx.path)?.module_type` |
| `package_json_unique_deps` | reads the file it's linting (it IS `package.json`) | unchanged — it's the primary target, not a consumer of `ProjectCtx`. Only the `CheckCtx` signature change propagates. |
| `package_json_sorted_deps` | same | same |

**Critical: use `nearest_*` accessors, not root fields.** Today `no_implicit_deps` walks up from the source file — in a monorepo, that's the *workspace package's* manifest, not the root's. Migrating to `ctx.project.package_json` (root only) silently regresses monorepo users. The `nearest_package_json(path)` accessor preserves current behaviour and caches results per manifest path.

The root-level `ctx.project.package_json` and `ctx.project.tsconfig` fields stay available for rules that genuinely want root-only data (e.g. framework detection). Manifest lookups scoped to the current file go through the `nearest_*` accessors.

So in practice 2 rules migrate to `ctx.project.nearest_*`, 2 keep reading the file they lint. All 4 pass through migration review — their `CheckCtx` shape changes, even when they don't use the new fields.

## 9. Testing

```rust
// All existing rule tests keep working unchanged:
fn run_ts(s: &str, check: &dyn AstCheck) -> Vec<Diagnostic> {
    let ctx = CheckCtx::for_test(Path::new("t.ts"), s);
    // for_test() synthesises default ProjectCtx + FileCtx
    check.check(&ctx, &tree)
}

// New: project-aware rule tests
#[test]
fn flags_use_client_in_server_component() {
    let project = ProjectCtx::for_test()
        .with_framework(Framework::NextJs);
    let ctx = CheckCtx::for_test_with_project(
        Path::new("app/page.tsx"),
        "export default function Page() { ... }",
        &project,
    );
    // file_ctx derived from path + source → ServerComponent
    assert_eq!(ctx.file.rsc_context, RscContext::ServerComponent);
}
```

Builder pattern on `ProjectCtx::for_test()` so tests declare only what they care about. Keeps migration of 4k existing tests mechanical (just `for_test` signature change).

## 10. Not Doing (and Why)

- **No TS config execution.** `tailwind.config.ts` with dynamic imports / spread / function calls is skipped, not evaluated. Reason: running user TS from a linter is a sandbox nightmare and a perf cliff. Static-only keeps comply sub-10s.
- **No Angular / NestJS framework variants.** Zero rules target them today. Add variants when the first rule asks.
- **No BitFlags for directives.** Two booleans (`use_client`, `use_server`) don't justify a new dependency. Revisit when we hit 6+ directives.
- **No cross-file index / symbol graph.** That's chantier #3. `ProjectCtx` is project-*configuration*, not project-*AST*.
- **No `.env`, `.gitignore`, `eslintrc`, `biome.json` parsing.** Out of scope — nothing in the current rule backlog needs them.
- **No lazy `package_json`.** It's <10KB, every rule run touches it; lazy would be premature.
- **No hot-reload on file watch.** `ProjectCtx` is built once per `lint_files` call. If the user edits `package.json` mid-run, they re-run comply. Watch mode (if ever added) rebuilds it on the next tick.
- **No `use strict` / `@ts-nocheck` / other directives in `FileDirectives`.** Keep it RSC-focused for now. Add when a rule needs them.

## 11. Assumption Audit

**Must Be True** (dealbreakers)
- `Arc<ProjectCtx>` is cheap to clone and share across rayon workers. *Validation: standard Rust pattern, proven by engine already sharing `Config` similarly.*
- `OnceLock<Option<T>>` races are safe under rayon. *Validation: `OnceLock` is `Sync`, std-documented.*
- Existing 4 rules' behaviour is preserved after migration. *Validation: their test suites run unchanged.*

**Should Be True** (adjustable)
- Project root detection via "common ancestor of lint targets, then walk up to nearest `package.json`" covers 95%+ of real projects. *If wrong: add an explicit `--project-root` CLI flag later.*
- Static-only Tailwind v3 parsing covers enough configs to be useful. *If wrong: rules opt out when theme is None; we don't regress.*

**Might Be True** (nice-to-have)
- Future framework additions fit the enum cleanly. *If wrong: break to `enum Framework { Known(KnownFramework), Other(String) }`.*

## 12. Open questions for implementation plan

- Should `DashMap` or `parking_lot::RwLock<HashMap>` back the `nearest_*` caches? Rayon contention pattern on first-hit dictates — benchmark the two before committing.
- Is `workspace_roots` detection via `package.json.workspaces` globs enough, or do we also need pnpm-workspace / Nx / Turborepo configs? (Likely defer to a follow-up PR.)
- Exact field set of `TailwindTheme` — colors + spacing today, but also radii? breakpoints? Add when a rule asks.

Behavioural decisions (project root detection, malformed manifest handling, entry-point wiring) are now fixed above — these remaining items are implementation-detail sizing questions.
