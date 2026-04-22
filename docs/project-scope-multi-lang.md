# Multi-Language `ImportIndex` — Design Note

> Date: 2026-04-22
> Status: Design proposal. Non-binding. Does not modify code.
> Context: Extends [`src/project/import_index.rs`](../src/project/import_index.rs) (currently TS/JS/TSX only) to every language comply targets: Rust, Vue SFC, SQL.
> Companion spec: [`docs/specs/2026-04-21-projectctx-filectx-design.md`](./specs/2026-04-21-projectctx-filectx-design.md).

---

## 1. Problem

Today's `ImportIndex`:

- Hard-codes TS/JS/TSX parsing via tree-sitter.
- Resolves relative specifiers with a fixed extension table (`.ts`, `.tsx`, `.js`, `.jsx`, `.mts`, `.mjs`) + `index.*` fallback.
- Uses `PathBuf` keyed `HashMap`s with a single shape of `ExportedSymbol` / `ImportedSymbol`.

It cannot answer questions like:

- "Is this Rust `pub fn` ever referenced outside its crate?" (dead `pub` surface)
- "Does this Vue component's `defineExpose` surface a method no parent calls?"
- "Which migration references the `users` table before it's created?"

Each new language currently would mean copy-pasting the whole indexer. Consumers would branch on `Language` themselves.

## 2. Consumer consequence

**Rule authors** write one rule that asks `ctx.project.imports()` and receives uniform `ExportedSymbol` / `ImportedSymbol` regardless of whether the file is `.rs`, `.vue`, or `.sql`. Language-specific fields sit behind an enum (`LanguageSpecific`) so rules that need them can pattern-match, while generic rules (unused exports, duplicate imports) stay language-agnostic.

**End users** get dead-code detection, circular-dep detection, and barrel-audit across their whole polyglot repo without shelling out to per-language tools (`cargo-udeps`, `knip`, `vue-tsc`, `sqlfluff`).

## 3. Proposed architecture

### 3.1 `LanguageIndexer` trait

Replace the monolithic TS-only parser with one indexer per language. Each implements a narrow trait; the top-level `ImportIndex::build` dispatches by `file.language`.

```rust
// src/project/import_index/mod.rs
pub trait LanguageIndexer: Send + Sync {
    /// Which `Language` this indexer handles.
    fn language(&self) -> Language;

    /// Parse `source` and emit exports + imports for this file.
    /// Parse failure returns `Default::default()` — never panic, never bubble.
    fn index_file(&self, path: &Path, source: &str, cfg: &IndexerConfig) -> FileIndex;

    /// Resolve a raw specifier to an absolute path within `known_files`.
    /// Returns `None` for bare specifiers that don't belong in the cross-file index.
    fn resolve(&self, from: &Path, specifier: &str, known: &FileSet) -> Option<PathBuf>;
}

pub struct FileIndex {
    pub exports: Vec<ExportedSymbol>,
    pub imports: Vec<ImportedSymbol>,
}
```

The existing TS/JS indexer becomes `TsIndexer: LanguageIndexer`. Rust / Vue / SQL each ship their own module.

### 3.2 Uniform symbol shape + language-specific payload

`ExportedSymbol` / `ImportedSymbol` keep their current shared fields (`name`, `line`, `specifier`, …) and grow one variant-typed field:

```rust
pub struct ExportedSymbol {
    pub name: String,
    pub kind: ExportKind,         // Default | Named | ReExport | StarReExport | …
    pub line: usize,
    pub reexport_source: Option<String>,
    pub extra: LanguageSpecific,  // NEW
}

pub enum LanguageSpecific {
    None,
    Rust(RustSymbol),
    Vue(VueSymbol),
    Sql(SqlSymbol),
}
```

Generic rules stay generic (`for exp in index.exports_of(path) { … }`). Language-specific rules match `extra` once and drop into their payload.

### 3.3 `ExportKind` is a closed enum — extend, don't split

```rust
pub enum ExportKind {
    Default,         // TS/JS only
    Named,           // all langs
    ReExport,        // TS/JS `export { x } from …`, Rust `pub use a::b`
    StarReExport,    // TS/JS `export * from …`, Rust `pub use a::*`
    // NEW
    Module,          // Rust `pub mod foo`
    Type,            // Rust `pub trait / struct / enum / type`
    SchemaObject,    // SQL `CREATE TABLE / VIEW / INDEX / FUNCTION / TRIGGER`
    ComponentExpose, // Vue `defineExpose({ open })`
    ComponentProp,   // Vue `defineProps<{…}>`
    ComponentEmit,   // Vue `defineEmits<{…}>`
}
```

Keep the enum non-exhaustive so adding `Elixir`, `Go`, etc. later is additive.

## 4. Rust implementation

### 4.1 Exports to detect

| Construct                 | tree-sitter node        | `ExportKind` | Notes |
|---------------------------|-------------------------|--------------|-------|
| `pub fn name(…)`          | `function_item`         | `Named`      | check `visibility_modifier` child is `pub` |
| `pub(crate) fn`           | `function_item`         | `Named`      | also export for same-crate indexing |
| `pub struct Foo`          | `struct_item`           | `Type`       |       |
| `pub enum Foo`            | `enum_item`             | `Type`       |       |
| `pub trait Foo`           | `trait_item`            | `Type`       |       |
| `pub type Foo = …`        | `type_item`             | `Type`       |       |
| `pub const / static`      | `const_item`, `static_item` | `Named`   |       |
| `pub mod foo`             | `mod_item`              | `Module`     | triggers child file resolution |
| `pub mod foo { … }`       | `mod_item` inline       | `Module`     | still an export; no file resolution |
| `pub use a::b::c`         | `use_declaration`       | `ReExport`   | `reexport_source = "a::b"`, `name = "c"` |
| `pub use a::*`            | `use_declaration`       | `StarReExport` |     |
| `pub use a::{b, c as d}`  | `use_declaration`       | `ReExport` × N | one symbol per tree-leaf |
| `macro_rules! foo` + `#[macro_export]` | `attribute_item` + `macro_definition` | `Named` | macros cross module boundaries via `#[macro_export]` |

Non-`pub` items (private `fn`, private `struct`) are skipped — they cannot be cross-file consumers.

### 4.2 Imports to detect

| Construct                    | tree-sitter node    | `ImportKind` | Notes |
|------------------------------|---------------------|--------------|-------|
| `use crate::foo::Bar`        | `use_declaration`   | `Named`      | anchored at crate root |
| `use super::foo`             | `use_declaration`   | `Named`      | anchored at parent `mod` |
| `use self::foo`              | `use_declaration`   | `Named`      | anchored at current `mod` |
| `use external::Thing`        | `use_declaration`   | `Named`      | bare, resolves via Cargo dep map (out of scope v1) |
| `use foo::{a, b as c}`       | `use_declaration`   | `Named` × N  | one import per leaf, `local_name = "c"`, `imported_name = "b"` |
| `use foo::*`                 | `use_declaration`   | `Namespace`  |       |
| `extern crate foo`           | `extern_crate_declaration` | `SideEffect` | legacy, rare |
| `mod foo;` (file-module)     | `mod_item` no body  | N/A — this is an *export*, not an import. It pulls `foo.rs` / `foo/mod.rs` into the crate tree. Index it as `ExportKind::Module` with a resolved `reexport_source`. |
| Fully-qualified calls (`crate::util::parse(…)`) | any `scoped_identifier` starting with `crate` / `super` / `self` | N/A v1 | Phase 2: inline-path usages without an explicit `use`. Common in Rust — skipping them misses real cross-file usage. |

### 4.3 Path resolution

Rust's module system is not a filesystem; it's declared with `mod foo;` statements. The indexer needs a two-pass build:

1. **Module graph pass.** Starting from `src/lib.rs` and `src/main.rs` (and each `tests/*.rs`, `benches/*.rs`, `examples/*.rs` as roots), traverse `mod foo;` declarations. For each declaration in `path/mod.rs` or `path/file.rs`:
   - Try `path/foo.rs` first.
   - Then `path/foo/mod.rs`.
   - Honour `#[path = "…"]` overrides on the `mod_item`.
   - Record `crate_path = "crate::a::b"` → `src/a/b.rs` (the module-path → file map).
2. **Symbol pass.** With the module-path map known, resolve every `use crate::a::b::Foo`:
   - Walk `a::b` to the file.
   - Bind `Foo` at that file's export list (directly exported, or re-exported via `pub use`).
   - `use super::` resolves against the current file's parent in the module tree.

Re-exports (`pub use a::b::Foo`) transitively forward a symbol through the current module. For usage tracking, the importing file sees `Foo` as coming from the re-exporting file; a rule that wants the original definition follows the `reexport_source` chain.

**Crate roots.** Single-crate: `src/lib.rs` xor `src/main.rs`. Cargo workspace: each member's `lib.rs` / `main.rs` is a separate root. `[[bin]]` / `[[example]]` / `[[test]]` / `[[bench]]` entries in `Cargo.toml` can override paths — v1 reads them lazily via `cargo_modules.rs` helpers already present in `src/`.

### 4.4 Resolution of bare crate specifiers (out of scope v1)

`use serde::Deserialize` points at an external crate. Indexing external crates is out of scope — same policy as TS `node_modules`. Rules that want "is this external dep declared in `Cargo.toml`?" go through `ctx.project.nearest_cargo_toml(path)` (separate field in `ProjectCtx`, parallel to `nearest_package_json`).

### 4.5 Example

```rust
// src/util/parse.rs
pub fn parse(s: &str) -> Result<T> { … }  // → ExportedSymbol { name: "parse", kind: Named, … }
fn helper() { … }                           // skipped — not pub

// src/lib.rs
pub mod util;                               // → ExportKind::Module, resolved to src/util/mod.rs
pub use util::parse;                        // → ExportKind::ReExport, reexport_source: Some("crate::util::parse")

// src/main.rs
use crate::util::parse::parse;              // → ImportedSymbol { imported_name: "parse", source_path: src/util/parse.rs }
```

### 4.6 Priority: **HIGH**

Rust-only rules already ship (28 in `rust_*`). The index unlocks cross-file Rust rules: dead `pub`, unused re-exports, circular `mod` dependencies, `pub use` shadowing. No other tool gives this on Cargo workspaces without full `rustc` integration.

---

## 5. Vue SFC implementation

### 5.1 File shape

A `.vue` file has up to three top-level blocks. Only `<script>` / `<script setup>` participate in the import/export graph; `<template>` consumes component identifiers via tag names; `<style>` can `@import` CSS but we don't index CSS dependencies.

### 5.2 Exports to detect (from `<script setup>`)

Vue's `<script setup>` compiles to an ESM module. Anything at the top level is locally bound; the compiler exposes three macro surfaces that act like exports to parent components:

| Macro                     | `ExportKind`        | `name`                    | Notes |
|---------------------------|---------------------|---------------------------|-------|
| `defineExpose({ open, … })` | `ComponentExpose` | one entry per key         | parent refs (`ref="child"`) can call them |
| `defineProps<{…}>()`      | `ComponentProp`     | one entry per prop key    | extracted from TS generic or options object |
| `defineEmits<{…}>()`      | `ComponentEmit`     | one entry per event name  | extracted from TS generic or string array |
| `defineModel<…>('name')`  | `ComponentProp` + `ComponentEmit` | `name` / `update:name` | Vue 3.4+ shorthand |
| top-level `export … from` | same as TS          | —                         | `<script setup>` rarely uses these; `<script>` (options API) can |

No `defineExpose` → the component exposes nothing to parents. The file still exists as a `default export` — Vue SFCs are ESM modules and parents `import Foo from './Foo.vue'`.

### 5.3 Imports to detect

Inside `<script setup>` / `<script>`, imports are plain ESM — reuse the TS indexer against the extracted script text. Two differences from plain TS:

- **Line mapping.** The script block is offset inside the `.vue` file. Track the starting line of `<script setup>` in the SFC and add it to each `ImportedSymbol.line` so diagnostics point at the right row.
- **Lang attribute.** `<script lang="ts">` / `<script lang="tsx">` switches the parser grammar. Default is JS.

### 5.4 Nuxt / Vue auto-imports (phase 2)

Nuxt auto-imports `ref`, `computed`, `useRoute`, composables from `composables/`, components from `components/` — without explicit `import`. Indexing these means:

1. Detect Nuxt via `ctx.project.framework == Framework::Nuxt`.
2. For each identifier referenced in the script but never imported, look it up in:
   - Built-in Nuxt / Vue macros (`ref`, `computed`, `useHead`, …) — known list.
   - `composables/*.ts` / `utils/*.ts` — files in the project with their default export named auto-imported.
   - `components/**/*.vue` — tag name → component file (Nuxt's naming rules, including nested dirs like `Base/Button.vue` → `<BaseButton>`).

Auto-imports show up in the index as `ImportedSymbol` with `specifier = "<auto:nuxt>"` and a resolved `source_path`, so existing rules see them like any other import.

### 5.5 Path resolution

Component imports usually carry `./Foo.vue` or `@/components/Foo.vue`. Resolution:

1. Resolve Vite / tsconfig aliases (`@/*`, `~/*`). Already planned in `ProjectCtx.tsconfig.paths`.
2. Try the specifier as-is with `.vue`, `.ts`, `.js`, `.mjs` extensions.
3. Try `/index.{vue,ts,js}`.

Same extension table as TS, plus `.vue`.

### 5.6 Priority: **MEDIUM**

4 Vue-specific text rules exist. Unlocking the import index enables:
- Unused `defineExpose` keys.
- Unused props / emits.
- Component imported but never used in template.
- Circular component imports.

These mirror high-value `react_*` rules but Vue's auto-import quirks (Nuxt) make v1 scope more painful than Rust. Recommend v1 covers only explicit imports; Nuxt auto-import resolution lands in v1.1.

---

## 6. SQL implementation

### 6.1 What "imports/exports" mean in SQL

SQL migrations don't have `import` / `export` statements. The dependency graph is implicit via DDL references:

- Migration creates a table → that table is "exported".
- Later migration adds a foreign key to that table → it "imports" the earlier migration's table.
- A migration references a table that no prior migration creates → dangling reference (probably a schema drift bug).

### 6.2 Exports to detect

| Statement                          | `ExportKind`   | `name` | `extra` payload |
|------------------------------------|----------------|--------|-----------------|
| `CREATE TABLE foo (…)`             | `SchemaObject` | `"foo"` | `SqlSymbol::Table { columns, constraints }` |
| `CREATE VIEW foo AS …`             | `SchemaObject` | `"foo"` | `SqlSymbol::View { depends_on: Vec<String> }` |
| `CREATE INDEX ix ON foo(col)`      | `SchemaObject` | `"ix"`  | `SqlSymbol::Index { on_table, on_columns }` |
| `CREATE FUNCTION foo(…)`           | `SchemaObject` | `"foo"` | `SqlSymbol::Function { args, return_type }` |
| `CREATE TRIGGER tr ON foo`         | `SchemaObject` | `"tr"`  | `SqlSymbol::Trigger { on_table }` |
| `CREATE TYPE foo AS ENUM(…)`       | `SchemaObject` | `"foo"` | `SqlSymbol::Type` |

### 6.3 Imports (implicit references)

| Reference                          | `ImportKind` | `imported_name` / `source_path` |
|------------------------------------|--------------|---------------------------------|
| `REFERENCES other_table(col)`      | `Named`      | table name, path = earlier migration that `CREATE TABLE`'d it |
| `ALTER TABLE other_table …`        | `Named`      | same |
| `DROP TABLE other_table`           | `Named`      | same |
| `FROM other_table` in a `CREATE VIEW` | `Named`   | same |
| `EXECUTE FUNCTION other_fn(…)` in a trigger | `Named` | same |

### 6.4 Ordering & migration resolution

Two variants, detected via directory layout:

- **Numeric prefixes** (`20240101_init.sql`, `20240102_add_users.sql`): sort by filename prefix. Migration N exports are visible to migration N+1+.
- **Drizzle / Prisma / sqlx `migrations/*.sql`**: same rule, varies on prefix convention.
- **Comply config fallback**: `ctx.project.config.sql.migrations_dir` (opt-in) tells the indexer where migrations live.

For a migration file, the "project" from its point of view is "all earlier migrations merged". The indexer builds a running schema snapshot in filename order; each file's imports resolve against that snapshot.

### 6.5 Parser

Use an existing SQL parser crate (tree-sitter SQL is incomplete for PG). Candidates:

- `sqlparser` (AST-level, pure Rust, already used by some comply rules — verify).
- `pg_query` (real PG parser via C FFI; heavier dep, most accurate).

Start with `sqlparser`; fall back to regex extraction for unparseable files (rare for migrations, common for ad-hoc queries).

### 6.6 What this does NOT cover v1

- Queries inside Rust/TS string literals (`sqlx::query!("SELECT …")`) — those remain TextCheck-style rules scanning string literals, not cross-file indexed.
- Non-migration SQL (stored procedures defined outside `migrations/`).
- Runtime-generated DDL (e.g., partitioning scripts).

### 6.7 Priority: **LOW**

SQL cross-migration dependency rules are valuable but narrow:
- "This migration references `users` before it exists."
- "This FK adds cascade on a high-write table without an index" (needs runtime knowledge too).
- "Renaming a table breaks a later migration's FK".

All doable, but the migration-ordering parse adds complexity. Recommend deferring until a rule actively requests it.

---

## 7. Consolidated `LanguageIndexer` registry

```rust
// src/project/import_index/mod.rs

pub struct ImportIndex {
    exports: HashMap<PathBuf, Vec<ExportedSymbol>>,
    imports: HashMap<PathBuf, Vec<ImportedSymbol>>,
    symbol_usages: HashMap<(PathBuf, String), Vec<Usage>>,
    rust_module_graph: Option<RustModuleGraph>,  // populated only if any .rs indexed
    sql_schema_at: Option<Vec<(PathBuf, SchemaSnapshot)>>, // ordered migrations
}

impl ImportIndex {
    pub fn build(files: &[&SourceFile], cfg: &Config, project: &ProjectCtx) -> Self {
        let indexers: Vec<Box<dyn LanguageIndexer>> = vec![
            Box::new(TsIndexer),
            Box::new(RustIndexer),
            Box::new(VueIndexer),
            Box::new(SqlIndexer),
        ];
        // 1. Group files by language.
        // 2. For langs with project-level pre-passes (Rust module graph, SQL migration ordering), run those first.
        // 3. Parallel per-file index_file calls via rayon.
        // 4. Resolve imports to absolute paths using each indexer's resolve().
        // 5. Backfill symbol_usages.
        …
    }
}
```

The per-language pre-passes are the only non-trivial plumbing beyond today's flat-parallel build. Each pre-pass has a clear cache seed: Rust's module graph, SQL's schema ladder. Neither blocks the other, so they run concurrently.

## 8. Testing

Each indexer ships with its own unit tests (`src/project/import_index/{rust,vue,sql}.rs` has a `#[cfg(test)] mod tests` block):

- Rust: `pub fn` / `pub use` / `mod` / `use crate::` coverage, re-export chain resolution, `#[path]` override, workspace roots.
- Vue: `<script setup>` extraction, `defineExpose` / `defineProps` / `defineEmits`, Nuxt auto-import resolution (v1.1).
- SQL: `CREATE TABLE` / FK references, migration ordering, dangling reference detection.

Integration test covers a polyglot fixture with a `.rs`, `.vue`, and `.sql` file each importing from each other's world to verify no cross-contamination.

## 9. Priorities & sequencing

| Phase | Scope | Effort | Unlocks |
|-------|-------|--------|---------|
| v1.0 (current) | TS/JS/TSX | done | TS unused-exports, barrel audit |
| v1.1 | **Rust** — explicit `use` + `pub` items + `mod` graph | ~1 week | Cross-file Rust rules (dead `pub`, unused `pub use`, circular `mod`) |
| v1.2 | **Vue** — explicit imports only (no Nuxt auto-import) | ~3 days | Unused `defineExpose`, unused props/emits, unused component imports |
| v1.3 | Vue Nuxt auto-import | ~1 week | Full Nuxt dead-code detection |
| v2.0 | **SQL** migrations — PG only via `sqlparser` | ~1 week | Migration dependency graph, dangling FK detection |

Rust first because: highest-value payoff (no tool does this without `rustc`), isolated from TS/Vue work, and the `rust_*` rule catalog already justifies the investment.

## 10. Open questions

- **Monorepo Cargo workspaces**: should the index treat each workspace member as an island, or cross-crate `use workspace_crate::…` as internal too? Likely "cross-crate internal" because comply is a single-project linter, but this duplicates `cargo check` work. Decide via first rule that asks.
- **Rust macro expansion**: `serde::Deserialize` derives inject `impl Deserialize for X`. The AST doesn't show those impls. Accept the blind spot, or run `cargo expand` (too slow)? v1: accept.
- **Vue `<script>` (options API) vs `<script setup>`**: options API exports via `export default { … }`. That "default" maps to the component; index it as `ExportKind::Default` with `LanguageSpecific::Vue` carrying `defineExpose`-equivalent (methods exposed in the object).
- **SQL dialect switching**: MySQL, SQLite, PG all have diverging DDL. Gate the parser on a `ctx.project.config.sql.dialect` setting, default PG.

---

## 11. Not doing

- No cross-language imports. Rust calling a stored SQL function is still two separate indexes; no edge bridges them.
- No runtime analysis (no `rustc`, no `vue-tsc`, no live DB introspection).
- No node_modules / Cargo registry resolution — bare specifiers stay unresolved, same policy as v1.0.
- No CSS `@import` indexing — style deps don't affect the correctness rules we care about.
