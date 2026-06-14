//! Cross-file import/export index built once per run.
//!
//! Rules that need cross-file visibility (unused exports, circular imports,
//! barrel-file detection) currently re-parse every file on demand or shell
//! out to Node-based tools (knip, madge). The in-process index lets native
//! rules answer "who imports `foo` from `src/util.ts`?" without either.
//!
//! How it works:
//! - `ImportIndex::build(files)` parses every indexable file once with
//!   tree-sitter. TS/JS/TSX are walked for `import_statement` /
//!   `export_statement`; Rust files are walked for `pub` items and `use`
//!   declarations.
//! - Exports are keyed by the absolute file path.
//! - For TS/JS: imports record the resolved absolute source path when the
//!   specifier is relative (`./foo`, `../bar`). Bare specifiers (`react`)
//!   are kept as-is.
//! - For Rust: `use crate::a::b::Sym` / `super::…` / `self::…` are resolved
//!   against a per-crate module graph rebuilt from `mod.rs` and `<name>.rs`
//!   conventions. External crates (`use serde::Deserialize`) stay
//!   unresolved.
//! - `symbol_usages` is computed by iterating imports after exports are
//!   known, linking each `(source_file, name)` pair to the importing sites.
//!
//! TS path resolution rules (relative specifiers only):
//! - `./foo` → `./foo.ts`, `./foo.tsx`, `./foo.js`, `./foo.jsx`,
//!   `./foo.mts`, `./foo.mjs`, `./foo/index.ts`, …
//! - First match wins; non-resolving specifiers are dropped from the index
//!   (they can't contribute cross-file usage anyway).
//!
//! Rust path resolution rules:
//! - `crate::` roots at the nearest `lib.rs` / `main.rs` ancestor.
//! - `super::` roots at the parent module of the importing file.
//! - `self::` roots at the module of the importing file.
//! - Each path segment is resolved via the module graph: a `mod foo;`
//!   declaration in `m.rs` looks for `foo.rs` or `foo/mod.rs` next to `m.rs`
//!   (or inside the directory of a crate root / `mod.rs`). The last segment
//!   is the symbol name in the resolved file.
//!
//! Limitations (deliberate):
//! - No node_modules resolution — bare specifiers are not cross-file indexed.
//! - `export * from './m'` records a re-export marker but does NOT transitively
//!   flatten symbols; consumers that need transitive export sets must handle
//!   that themselves.
//! - Rust `mod foo { … }` inline modules are not tracked; only file-backed
//!   modules (`mod foo;`) participate in the module graph.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use oxc_resolver::{
    ResolveOptions, Resolver, TsconfigDiscovery, TsconfigOptions, TsconfigReferences,
};
use rayon::prelude::*;
use tree_sitter::{Node, Parser};

use crate::files::{Language, SourceFile};
use crate::rules::no_implicit_deps::is_bare_specifier;
use crate::rules::walker::walk_tree;

/// Kind of an exported symbol. Tracks syntactic shape, not type — `Function`
/// means it was declared with `function`, `Class` with `class`, etc. `Value`
/// covers `const`/`let`/`var` bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// `export default …`
    Default,
    /// `function` / `class` / `const …` / `let …` / `var …` / `enum` / `type` / `interface`.
    Named,
    /// `export { name } from './m'` or `export * as ns from './m'`.
    ReExport,
    /// `export * from './m'` — no specific name, re-exports everything.
    StarReExport,
    /// Rust-only: `pub mod foo;` — makes a submodule visible. Has no
    /// TS/JS analogue (module declarations don't re-export symbols on their
    /// own; they expose the submodule as a namespace).
    Module,
}

/// One exported symbol at a source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedSymbol {
    /// Local name visible to importers. For `export default` this is
    /// `"default"` regardless of the original identifier.
    pub name: String,
    pub kind: ExportKind,
    /// 1-based line number of the `export` keyword.
    pub line: usize,
    /// Only populated for `ReExport` / `StarReExport` — the (possibly
    /// resolved) source path or raw specifier.
    pub reexport_source: Option<String>,
    /// Parameter names for function exports (empty for non-functions).
    pub params: Vec<String>,
    /// `true` for `export type Foo` / `export interface Foo` — pure type
    /// exports that live only in the TypeScript namespace and emit no runtime
    /// JS. TypeScript allows `export const X` and `export type X` to coexist
    /// under the same name (value namespace vs. type namespace), so callers
    /// must group by `(name, is_type_only)` rather than `name` alone.
    pub is_type_only: bool,
    /// Source binding for `export { local as exported }` (and the re-export
    /// form `export { local as exported } from './m'`): the `local` side, when
    /// it differs from `name`. `None` for declaration exports and plain
    /// `export { x }`. Lets callers tell that the `default` export is an alias
    /// of a named binding (`export { Foo as default }`) rather than an
    /// unrelated default that merely coexists with a named `Foo`.
    pub local_name: Option<String>,
}

/// Kind of an imported symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// `import foo from '…'`
    Default,
    /// `import { foo } from '…'` / `import { foo as bar } from '…'`
    Named,
    /// `import * as foo from '…'`
    Namespace,
    /// `import '…'` — side effect only.
    SideEffect,
}

/// One imported symbol at a source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSymbol {
    /// Local binding name in the importing file. For `import { a as b }`
    /// this is `"b"`. For side-effect imports this is empty.
    pub local_name: String,
    /// Original export name at the source — for `import { a as b }` this is
    /// `"a"`. Equal to `local_name` when no renaming occurred.
    pub imported_name: String,
    pub kind: ImportKind,
    /// Raw specifier as it appears in source (`'./foo'`, `'react'`).
    pub specifier: String,
    /// Absolute resolved path when `specifier` is relative and resolves to a
    /// file that exists in the input set. `None` for bare specifiers.
    pub source_path: Option<PathBuf>,
    /// 1-based line number of the `import` keyword.
    pub line: usize,
    /// `import type { X }` or `import { type X }` — value never needed at runtime.
    pub is_type_only: bool,
}

/// One use-site of a cross-file exported symbol — i.e. a matching import.
#[derive(Debug, Clone)]
pub struct Usage {
    /// Absolute path of the file that imports the symbol.
    pub importer: PathBuf,
    /// Local binding name in the importer (may differ from the exported name
    /// via `as` renaming).
    pub local_name: String,
    /// 1-based line number of the import.
    pub line: usize,
}

/// Whether an identifier reference is invoked with `new` or as a plain call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    /// `new Foo(...)`
    New,
    /// `Foo(...)`
    Call,
}

/// A concrete call-site of a cross-file exported symbol in an importing file.
/// Distinct from `Usage` (which is per-import-statement) — one import can
/// produce many call sites.
#[derive(Debug, Clone)]
pub struct CallSite {
    /// Absolute path of the file containing the call.
    pub path: PathBuf,
    /// 1-based line number of the call expression.
    pub line: usize,
    /// 1-based column of the call expression.
    pub column: usize,
    /// Byte offset of the call expression in the file.
    pub byte_offset: usize,
    /// Byte length of the call expression.
    pub byte_len: usize,
    pub kind: CallKind,
    /// Argument names at call site (None if not a simple identifier).
    pub args: Vec<Option<String>>,
}

/// Aggregated info about a bare specifier (npm package) across all files.
#[derive(Debug, Clone, Default)]
pub struct BareSpecifierInfo {
    /// Every import from this package is `import type` — no runtime dependency.
    pub type_only: bool,
    /// Files that import this package.
    pub importers: Vec<PathBuf>,
}

/// Snapshot of exports, imports, and cross-file symbol usages for the input
/// set. Frozen after `build` — all fields are read-only for rule consumers.
#[derive(Debug, Default)]
pub struct ImportIndex {
    exports: HashMap<PathBuf, Vec<ExportedSymbol>>,
    imports: HashMap<PathBuf, Vec<ImportedSymbol>>,
    /// `(exporting_file, exported_name)` → every importer that pulls it in.
    /// Populated after re-export propagation so barrel usages flow through.
    symbol_usages: HashMap<(PathBuf, String), Vec<Usage>>,
    /// `(exporting_file, exported_name)` → every cross-file call/new site that
    /// references it. Populated only for named + default imports that resolve
    /// to a known exporting file. Namespace imports (`import * as ns`) and
    /// member-access calls (`ns.Foo()`) are not tracked.
    call_sites: HashMap<(PathBuf, String), Vec<CallSite>>,
    /// Bare specifiers (npm package names) seen across all files, mapped to
    /// whether ALL imports from that package are type-only.
    bare_specifiers: HashMap<String, BareSpecifierInfo>,
    /// Strongly connected components with >1 member (import cycles).
    cycles: Vec<Vec<PathBuf>>,
    /// Raw discovered path → canonical absolute path. Built from the
    /// canonicalization already performed during extraction, so cross-file
    /// rules look up `ctx.path`'s canonical form with an O(1) map hit instead
    /// of a per-file `std::fs::canonicalize` syscall.
    canonical: HashMap<PathBuf, PathBuf>,
    /// Distinct importing files per exporting (canonical) path. Precomputed
    /// reverse edge of `imports` so `get_importers` / `importer_count` are
    /// O(1) lookups rather than a full scan of every file's imports.
    importers: HashMap<PathBuf, Vec<PathBuf>>,
    /// Lexicographically smallest indexed (canonical) path, or `None` when no
    /// file was indexed. Once-per-project rules anchor on this; precomputing it
    /// avoids an O(N) `indexed_paths().min()` scan on every file.
    min_indexed: Option<PathBuf>,
    /// Canonical paths reached by at least one namespace import
    /// (`import * as ns from './m'`). `dead-export` treats every export of such
    /// a module as live; precomputing the set avoids an O(N) `get_imports_to`
    /// scan per file.
    namespace_imported: std::collections::HashSet<PathBuf>,
    /// `(re-exporting file, exported name)` → resolved origin file of a
    /// `export { name } from './m'`. Only relative specifiers that resolve to an
    /// indexed file are recorded. Lets rules tell a re-export *chain* (one
    /// barrel re-exporting through another) from independent barrels that both
    /// re-export the same name.
    reexport_targets: HashMap<(PathBuf, String), PathBuf>,
    /// Re-exporting file → resolved source files of its `export … from './m'`
    /// declarations (both `export { x } from` and `export * from`). A barrel
    /// reached through the import graph keeps the files it re-exports reachable,
    /// so reachability traversal follows these edges as well as import edges.
    reexport_edges: HashMap<PathBuf, Vec<PathBuf>>,
    /// Canonical directories referenced by a template-literal dynamic import
    /// (`import(\`./locales/${lang}\`)`). The runtime path is computed, so any
    /// file under such a directory is reachable and its exports are live —
    /// `unused-file` and `dead-export` must not flag them.
    dynamic_import_dirs: Vec<PathBuf>,
}

impl ImportIndex {
    /// Parse every TS/JS/TSX/Rust file in `files` and build the index. Vue
    /// files are ignored (Vue `<script>` blocks are not yet extracted).
    #[must_use]
    pub fn build(files: &[&SourceFile]) -> Self {
        // Per-file parse + extract runs in parallel; each worker gets its own
        // `Parser` because `tree_sitter::Parser` is !Sync. `map_init` is the
        // same pattern the engine already uses for rule dispatch.
        let per_file_raw: Vec<(PathBuf, PathBuf, FileExtract)> = files
            .par_iter()
            .filter(|f| is_indexable(f.language))
            .map_init(Parser::new, |parser, file| {
                extract_for(parser, file).map(|(canon, extract)| (file.path.clone(), canon, extract))
            })
            .flatten()
            .collect();

        // Raw → canonical map, reusing the canonicalization extract_for already
        // did. Lets cross-file rules canonicalize `ctx.path` with a map hit.
        let mut canonical: HashMap<PathBuf, PathBuf> =
            HashMap::with_capacity(per_file_raw.len());
        let per_file: Vec<(PathBuf, FileExtract)> = per_file_raw
            .into_iter()
            .map(|(raw, canon, extract)| {
                canonical.insert(raw, canon.clone());
                (canon, extract)
            })
            .collect();

        let mut exports: HashMap<PathBuf, Vec<ExportedSymbol>> = HashMap::new();
        let mut imports: HashMap<PathBuf, Vec<ImportedSymbol>> = HashMap::new();
        let mut file_calls: HashMap<PathBuf, Vec<LocalCall>> = HashMap::new();
        let known_paths: std::collections::HashSet<PathBuf> =
            per_file.iter().map(|(p, _)| p.clone()).collect();

        // Rust resolution is more involved than TS: specifiers are not file
        // paths but module paths (`crate::a::b::Sym`) that need a module graph
        // built from `mod foo;` declarations. Build it once here.
        let rust_graph = RustModuleGraph::build(&per_file, &known_paths);

        // Load module resolver (tsconfig paths + node_modules) for TS resolution.
        // Discovers all tsconfigs reachable from the indexed files so each
        // sub-project gets its own path aliases.
        let path_resolver = OxcPathResolver::discover(&known_paths);

        // First pass: resolve each import's specifier to a concrete file. This
        // is disk-bound — relative-specifier resolution stat-probes `./foo.ts`,
        // `./foo/index.ts`, … — and fully independent per file, so run it across
        // the rayon pool. The resolvers it reads (`rust_graph`, `known_paths`,
        // `path_resolver`) are immutable and `Sync`. Map insertion below stays
        // sequential: it's pure in-memory work and would only contend on locks.
        let resolved: Vec<(PathBuf, FileExtract, Vec<PathBuf>)> = per_file
            .into_par_iter()
            .map(|(path, mut extract)| {
                let is_rust = matches!(path.extension().and_then(|e| e.to_str()), Some("rs"));
                for imp in &mut extract.imports {
                    if is_rust {
                        if let Some(resolved) = rust_graph.resolve(&path, &imp.specifier) {
                            imp.source_path = Some(resolved);
                        }
                    } else if let Some(resolved) =
                        resolve_specifier(&path, &imp.specifier, &known_paths, &path_resolver)
                    {
                        imp.source_path = Some(resolved);
                    }
                }
                // Resolve each template-literal dynamic-import directory prefix
                // against the importer's directory into a canonical directory.
                let dyn_dirs: Vec<PathBuf> = extract
                    .dynamic_dirs
                    .iter()
                    .filter_map(|rel| resolve_dynamic_dir(&path, rel))
                    .collect();
                (path, extract, dyn_dirs)
            })
            .collect();

        let mut dynamic_import_dirs: Vec<PathBuf> = Vec::new();
        for (path, extract, dyn_dirs) in resolved {
            exports.insert(path.clone(), extract.exports);
            imports.insert(path.clone(), extract.imports);
            file_calls.insert(path, extract.calls);
            dynamic_import_dirs.extend(dyn_dirs);
        }
        dynamic_import_dirs.sort();
        dynamic_import_dirs.dedup();

        // Second pass: link imports → exports via symbol_usages. Only named
        // and default imports link cleanly; namespace imports touch every
        // export and are left to callers (we'd otherwise balloon the map).
        let mut symbol_usages: HashMap<(PathBuf, String), Vec<Usage>> = HashMap::new();
        for (importer, imps) in &imports {
            for imp in imps {
                let Some(src) = &imp.source_path else {
                    continue;
                };
                let exported_name = match imp.kind {
                    ImportKind::Default => "default".to_string(),
                    ImportKind::Named => imp.imported_name.clone(),
                    ImportKind::Namespace | ImportKind::SideEffect => continue,
                };
                symbol_usages
                    .entry((src.clone(), exported_name))
                    .or_default()
                    .push(Usage {
                        importer: importer.clone(),
                        local_name: imp.local_name.clone(),
                        line: imp.line,
                    });
            }
        }

        // Third pass: link call-sites → exports via call_sites. For each
        // importer, build a map (local_name → (exporting_file, exported_name))
        // from its named/default imports, then translate each collected
        // `new Foo(...)` / `Foo(...)` whose callee matches a local binding.
        let mut call_sites: HashMap<(PathBuf, String), Vec<CallSite>> = HashMap::new();
        for (importer, imps) in &imports {
            let Some(calls) = file_calls.get(importer) else {
                continue;
            };
            let mut local_to_source: HashMap<String, (PathBuf, String)> = HashMap::new();
            for imp in imps {
                let Some(src) = &imp.source_path else {
                    continue;
                };
                let exported_name = match imp.kind {
                    ImportKind::Default => "default".to_string(),
                    ImportKind::Named => imp.imported_name.clone(),
                    ImportKind::Namespace | ImportKind::SideEffect => continue,
                };
                local_to_source.insert(imp.local_name.clone(), (src.clone(), exported_name));
            }
            if local_to_source.is_empty() {
                continue;
            }
            for call in calls {
                let Some((src, exported)) = local_to_source.get(&call.local_name) else {
                    continue;
                };
                call_sites
                    .entry((src.clone(), exported.clone()))
                    .or_default()
                    .push(CallSite {
                        path: importer.clone(),
                        line: call.line,
                        column: call.column,
                        byte_offset: call.byte_offset,
                        byte_len: call.byte_len,
                        kind: call.kind,
                        args: call.args.clone(),
                    });
            }
        }

        // Resolve each `export … from './m'` to its origin file. Reuses the
        // same specifier resolver as imports so tsconfig aliases and extension
        // probing stay consistent. Only relative specifiers that land on an
        // indexed file are recorded. `star_edges` keeps the `export * from`
        // subset so re-export propagation can flow barrel usages into the
        // wildcard-re-exported origins (named edges are matched separately by
        // `propagate_reexports`).
        let mut reexport_targets: HashMap<(PathBuf, String), PathBuf> = HashMap::new();
        let mut reexport_edges: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        let mut star_edges: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        for (path, exps) in &exports {
            for exp in exps {
                if !matches!(exp.kind, ExportKind::ReExport | ExportKind::StarReExport) {
                    continue;
                }
                let Some(spec) = &exp.reexport_source else {
                    continue;
                };
                let Some(origin) = resolve_specifier(path, spec, &known_paths, &path_resolver)
                else {
                    continue;
                };
                if matches!(exp.kind, ExportKind::ReExport) {
                    reexport_targets.insert((path.clone(), exp.name.clone()), origin.clone());
                } else {
                    let star = star_edges.entry(path.clone()).or_default();
                    if !star.contains(&origin) {
                        star.push(origin.clone());
                    }
                }
                let edges = reexport_edges.entry(path.clone()).or_default();
                if !edges.contains(&origin) {
                    edges.push(origin);
                }
            }
        }

        // Fourth pass: propagate re-exports. When barrel.ts does
        // `export { X } from './impl'`, usages on barrel flow to impl; the same
        // applies to `export * from './impl'` for whichever name `impl` exports.
        propagate_reexports(&exports, &imports, &star_edges, &mut symbol_usages);

        // Fifth pass: collect bare specifiers (npm packages).
        let bare_specifiers = collect_bare_specifiers(&imports);

        // Sixth pass: Tarjan SCC for cycle detection.
        let cycles = compute_cycles(&imports);

        // Reverse edge: distinct importing files per exporting path. One entry
        // per (source, importer) pair — multiple imports of the same source
        // from one file collapse to a single importer (matching the old
        // `get_importers` scan semantics).
        let mut importers: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        let mut namespace_imported: std::collections::HashSet<PathBuf> =
            std::collections::HashSet::new();
        for (importer, imps) in &imports {
            let mut seen: std::collections::HashSet<&Path> = std::collections::HashSet::new();
            for imp in imps {
                if let Some(src) = &imp.source_path {
                    if seen.insert(src.as_path()) {
                        importers.entry(src.clone()).or_default().push(importer.clone());
                    }
                    if imp.kind == ImportKind::Namespace {
                        namespace_imported.insert(src.clone());
                    }
                }
            }
        }

        let min_indexed = exports.keys().min().cloned();

        Self {
            exports,
            imports,
            symbol_usages,
            call_sites,
            bare_specifiers,
            cycles,
            canonical,
            importers,
            min_indexed,
            namespace_imported,
            reexport_targets,
            reexport_edges,
            dynamic_import_dirs,
        }
    }

    /// Exports declared in `path`, or empty slice if the file isn't indexed.
    #[must_use]
    pub fn get_exports(&self, path: &Path) -> &[ExportedSymbol] {
        self.exports.get(path).map_or(&[], Vec::as_slice)
    }

    /// Imports declared in `path`, or empty slice if the file isn't indexed.
    #[must_use]
    pub fn get_imports(&self, path: &Path) -> &[ImportedSymbol] {
        self.imports.get(path).map_or(&[], Vec::as_slice)
    }

    /// `true` when `target` is the only indexed source file living directly in
    /// its parent directory (files in subdirectories are not counted). Such a
    /// lone `index` is the module itself organized across its own subtree, not
    /// a hub aggregating sibling modules.
    #[must_use]
    pub fn is_sole_file_in_dir(&self, target: &Path) -> bool {
        let Some(dir) = target.parent() else {
            return false;
        };
        !self
            .exports
            .keys()
            .any(|p| p.as_path() != target && p.parent() == Some(dir))
    }

    /// Every importer pulling `symbol` from `path`. Empty slice when the
    /// symbol is unused across the indexed file set.
    #[must_use]
    pub fn get_usages(&self, path: &Path, symbol: &str) -> &[Usage] {
        self.symbol_usages
            .get(&(path.to_path_buf(), symbol.to_string()))
            .map_or(&[], Vec::as_slice)
    }

    /// Every cross-file call/new expression that references `symbol` exported
    /// from `path`. Distinct from `get_usages`: `get_usages` returns one entry
    /// per import statement, whereas this returns one entry per call-site.
    #[must_use]
    pub fn get_call_sites(&self, path: &Path, symbol: &str) -> &[CallSite] {
        self.call_sites
            .get(&(path.to_path_buf(), symbol.to_string()))
            .map_or(&[], Vec::as_slice)
    }

    /// Convenience: files that import from `path` at all (any symbol).
    #[must_use]
    pub fn get_importers(&self, path: &Path) -> Vec<&Path> {
        self.importers
            .get(path)
            .map(|v| v.iter().map(PathBuf::as_path).collect())
            .unwrap_or_default()
    }

    /// Number of distinct files importing from `path`. O(1) lookup against the
    /// precomputed reverse-edge map.
    #[must_use]
    pub fn importer_count(&self, path: &Path) -> usize {
        self.importers.get(path).map_or(0, Vec::len)
    }

    /// Canonical absolute path for a discovered file path. Uses the raw →
    /// canonical map built during indexing (zero syscalls); falls back to a
    /// `std::fs::canonicalize` for paths absent from the index — the LSP
    /// single-file path and unit tests that construct `CheckCtx` by hand.
    #[must_use]
    pub fn canonical(&self, path: &Path) -> PathBuf {
        if let Some(c) = self.canonical.get(path) {
            return c.clone();
        }
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    /// Total number of indexed files — the denominator for ratio-based rules.
    #[must_use]
    pub fn total_files(&self) -> usize {
        self.exports.len()
    }

    /// Lexicographically smallest indexed (canonical) path. Precomputed once;
    /// cross-file rules use it as a deterministic per-run anchor.
    #[must_use]
    pub fn min_indexed_path(&self) -> Option<&Path> {
        self.min_indexed.as_deref()
    }

    /// True when `path` is reached by at least one namespace import
    /// (`import * as ns from …`). O(1) set lookup.
    #[must_use]
    pub fn is_namespace_imported(&self, path: &Path) -> bool {
        self.namespace_imported.contains(path)
    }

    /// Every import site across the project that resolves to `path`.
    /// Unlike `get_importers` (one entry per importing file), this yields
    /// one entry per actual import statement and exposes `ImportKind` —
    /// rules that need to distinguish named from namespace imports consume
    /// this instead (e.g. `dead-export` treats namespace importers as
    /// "uses everything" because the per-name usage map doesn't populate
    /// for `import * as ns`).
    #[must_use]
    pub fn get_imports_to(&self, path: &Path) -> Vec<&ImportedSymbol> {
        let mut out = Vec::new();
        for imps in self.imports.values() {
            for imp in imps {
                if imp.source_path.as_deref() == Some(path) {
                    out.push(imp);
                }
            }
        }
        out
    }

    /// True if `symbol` is exported by at least one indexed file.
    #[must_use]
    pub fn is_exported_anywhere(&self, symbol: &str) -> bool {
        self.exports
            .values()
            .any(|v| v.iter().any(|e| e.name == symbol))
    }

    /// Iterate every (path, exports) pair — used by rules that need to walk
    /// the full export surface (e.g. barrel-file detection).
    pub fn iter_exports(&self) -> impl Iterator<Item = (&Path, &[ExportedSymbol])> {
        self.exports
            .iter()
            .map(|(p, v)| (p.as_path(), v.as_slice()))
    }

    /// Iterate every imported symbol across all indexed files. Lets rules
    /// inspect import specifiers the index could not resolve to a concrete
    /// file (`source_path == None`), e.g. build-tool path aliases.
    pub fn iter_imports(&self) -> impl Iterator<Item = &ImportedSymbol> {
        self.imports.values().flat_map(|v| v.iter())
    }

    /// Bare specifiers (npm package names) collected from all imports.
    #[must_use]
    pub fn bare_specifiers(&self) -> &HashMap<String, BareSpecifierInfo> {
        &self.bare_specifiers
    }

    /// Resolved origin file of `export { name } from './m'` declared in `path`.
    /// `None` when the file declares no such re-export of `name`, or its
    /// specifier is bare / doesn't resolve to an indexed file.
    #[must_use]
    pub fn reexport_target(&self, path: &Path, name: &str) -> Option<&Path> {
        self.reexport_targets
            .get(&(path.to_path_buf(), name.to_string()))
            .map(PathBuf::as_path)
    }

    /// Files reachable from `roots` via import edges (BFS). Unreachable files
    /// in the indexed set are candidates for the `unused-file` rule.
    #[must_use]
    pub fn reachable_from(&self, roots: &[&Path]) -> std::collections::HashSet<PathBuf> {
        let mut visited = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<PathBuf> =
            roots.iter().map(|p| p.to_path_buf()).collect();
        // Any file under a template-literal dynamic-import directory is reachable
        // at runtime: the substitution path resolves to one of these files, but
        // statically we cannot tell which, so the whole directory is referenced.
        for path in self.exports.keys() {
            if self.is_under_dynamic_import_dir(path) {
                queue.push_back(path.clone());
            }
        }
        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }
            for imp in self.get_imports(&current) {
                if let Some(src) = &imp.source_path
                    && !visited.contains(src)
                {
                    queue.push_back(src.clone());
                }
            }
            // A barrel reached here re-exports through `export … from './m'`;
            // its source files are reachable too.
            if let Some(edges) = self.reexport_edges.get(&current) {
                for src in edges {
                    if !visited.contains(src) {
                        queue.push_back(src.clone());
                    }
                }
            }
        }
        visited
    }

    /// True when `path` lives under a directory referenced by a template-literal
    /// dynamic import (`import(\`./locales/${lang}\`)`). The runtime path is
    /// computed, so any file under such a directory is reachable and every
    /// export of it is live — `unused-file` and `dead-export` must not flag it.
    #[must_use]
    pub fn is_under_dynamic_import_dir(&self, path: &Path) -> bool {
        self.dynamic_import_dirs
            .iter()
            .any(|dir| path.starts_with(dir))
    }

    /// All import cycles (SCCs with 2+ members), computed once via Tarjan.
    #[must_use]
    pub fn cycles(&self) -> &[Vec<PathBuf>] {
        &self.cycles
    }

    /// Cycle containing `path`, if any.
    #[must_use]
    pub fn cycle_for(&self, path: &Path) -> Option<&[PathBuf]> {
        self.cycles
            .iter()
            .find(|scc| scc.iter().any(|p| p == path))
            .map(|v| v.as_slice())
    }

    /// True when no TS/JS/TSX file was indexed. Cross-file rules use this
    /// to short-circuit in contexts that don't build a real index — the LSP
    /// path and unit tests constructed via `CheckCtx::for_test`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.exports.is_empty() && self.imports.is_empty()
    }

    /// Every indexed TS/JS/TSX file path. Cross-file rules that need to
    /// re-parse every participant (e.g. `no-identical-functions`) iterate
    /// this — the index is the only place the per-run file set is retained
    /// after `ProjectCtx::load` returns.
    pub fn indexed_paths(&self) -> impl Iterator<Item = &Path> {
        self.exports.keys().map(PathBuf::as_path)
    }
}

/// Fixed-point propagation: when `barrel.ts` re-exports `{ X } from './impl'`,
/// any usage of `X` on barrel should count as a usage of `X` on impl. The same
/// holds for `export * from './impl'`: a usage of `X` on barrel flows to impl
/// when impl exports `X`, since the wildcard re-export is the only thing that
/// makes `import { X } from './barrel'` resolve.
fn propagate_reexports(
    exports: &HashMap<PathBuf, Vec<ExportedSymbol>>,
    imports: &HashMap<PathBuf, Vec<ImportedSymbol>>,
    star_edges: &HashMap<PathBuf, Vec<PathBuf>>,
    symbol_usages: &mut HashMap<(PathBuf, String), Vec<Usage>>,
) {
    // Build re-export edges: for each `export { X } from './m'` or
    // `export { X as Y } from './m'`, find the matching import and link
    // (barrel, exported_name) → (origin, origin_name).
    //
    // Matching strategy: the re-export's `reexport_source` specifier
    // must match the import's `specifier`, AND the names must align.
    // This avoids false edges when a barrel imports the same name from
    // multiple modules.
    let mut reexport_edges: Vec<(PathBuf, String, PathBuf, String)> = Vec::new();
    for (barrel_path, exps) in exports {
        let Some(imps) = imports.get(barrel_path) else {
            continue;
        };
        for exp in exps {
            if !matches!(exp.kind, ExportKind::ReExport) {
                continue;
            }
            let Some(reexport_spec) = &exp.reexport_source else {
                continue;
            };
            for imp in imps {
                if imp.specifier != *reexport_spec {
                    continue;
                }
                let Some(origin) = &imp.source_path else {
                    continue;
                };
                let name_matches = match imp.kind {
                    ImportKind::Named => imp.local_name == exp.name,
                    ImportKind::Default => exp.name == "default",
                    _ => false,
                };
                if name_matches {
                    let origin_name = match imp.kind {
                        ImportKind::Default => "default".to_string(),
                        _ => imp.imported_name.clone(),
                    };
                    reexport_edges.push((
                        barrel_path.clone(),
                        exp.name.clone(),
                        origin.clone(),
                        origin_name,
                    ));
                    break;
                }
            }
        }
    }

    // Star re-export edges: for each `export * from './m'` on a barrel, a usage
    // of name `X` on the barrel is a usage of `X` on whichever module in the
    // star chain actually declares `X`. Walk the star-edge graph from each
    // barrel to its terminal modules, then link (barrel, X) → (terminal, X) for
    // every runtime name the terminal exports. Following the chain to the
    // declaring file (rather than each intermediate barrel) lets a single edge
    // cover nested `export *` barrels without those intermediates needing the
    // name in their own export list.
    for barrel_path in star_edges.keys() {
        for (name, terminal) in star_reexported_names(barrel_path, star_edges, exports) {
            reexport_edges.push((barrel_path.clone(), name.clone(), terminal, name));
        }
    }

    // Fixed-point: propagate usages through re-export chains.
    let mut changed = true;
    let mut iterations = 0;
    while changed && iterations < 20 {
        changed = false;
        iterations += 1;
        for (barrel, barrel_name, origin, origin_name) in &reexport_edges {
            let barrel_usages = symbol_usages
                .get(&(barrel.clone(), barrel_name.clone()))
                .cloned()
                .unwrap_or_default();
            if barrel_usages.is_empty() {
                continue;
            }
            let origin_usages = symbol_usages
                .entry((origin.clone(), origin_name.clone()))
                .or_default();
            for usage in &barrel_usages {
                if !origin_usages
                    .iter()
                    .any(|u| u.importer == usage.importer && u.line == usage.line)
                {
                    origin_usages.push(usage.clone());
                    changed = true;
                }
            }
        }
    }
}

/// Runtime export names made available on `barrel` through its `export * from`
/// chain, paired with the module that actually declares each name. Walks
/// `star_edges` breadth-first from `barrel`; at each visited module it records
/// the module's own non-star exports (a name closer to the barrel in the chain
/// shadows a deeper one, matching JS `export *` resolution) and follows that
/// module's further star edges. Type-only exports are skipped — they leave no
/// runtime symbol for `dead-export` to reason about.
fn star_reexported_names(
    barrel: &Path,
    star_edges: &HashMap<PathBuf, Vec<PathBuf>>,
    exports: &HashMap<PathBuf, Vec<ExportedSymbol>>,
) -> Vec<(String, PathBuf)> {
    let mut resolved: HashMap<String, PathBuf> = HashMap::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut queue: std::collections::VecDeque<PathBuf> = star_edges
        .get(barrel)
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    while let Some(module) = queue.pop_front() {
        if !visited.insert(module.clone()) {
            continue;
        }
        for exp in exports.get(&module).map_or(&[][..], Vec::as_slice) {
            if matches!(exp.kind, ExportKind::StarReExport) {
                continue;
            }
            resolved.entry(exp.name.clone()).or_insert_with(|| module.clone());
        }
        if let Some(next) = star_edges.get(&module) {
            for src in next {
                if !visited.contains(src) {
                    queue.push_back(src.clone());
                }
            }
        }
    }
    resolved.into_iter().collect()
}

/// Extract bare specifier → package name mapping from all imports.
/// `@scope/pkg/path` → `@scope/pkg`, `lodash/fp` → `lodash`.
fn collect_bare_specifiers(
    imports: &HashMap<PathBuf, Vec<ImportedSymbol>>,
) -> HashMap<String, BareSpecifierInfo> {
    let mut result: HashMap<String, BareSpecifierInfo> = HashMap::new();
    // Per-package importer dedup. The previous `entry.importers.contains(file)`
    // scan was O(importers²) per package — quadratic for a dependency imported
    // by thousands of files. A `HashSet` makes the membership test O(1) while
    // preserving the first-seen insertion order into the `Vec`.
    let mut seen: HashMap<String, std::collections::HashSet<PathBuf>> = HashMap::new();
    for (file, imps) in imports {
        // Rust external crates that weren't resolved by RustModuleGraph are
        // not npm packages — skip them to avoid "unlisted-dependency" FPs.
        if file.extension().is_some_and(|e| e == "rs") {
            continue;
        }
        for imp in imps {
            // Skip imports resolved to a local file and any specifier that is
            // not a bare package name (relative/absolute paths, URL imports).
            // `extract_package_name` splits on `/`, so a URL like
            // `https://cdn/pkg.js` would otherwise yield the bogus package
            // `https:` and surface as an unlisted dependency.
            if imp.source_path.is_some() || !is_bare_specifier(&imp.specifier) {
                continue;
            }
            let pkg = extract_package_name(&imp.specifier);
            if pkg.is_empty() || is_builtin_module(&pkg) {
                continue;
            }
            let is_new_importer = seen.entry(pkg.clone()).or_default().insert(file.clone());
            let entry = result.entry(pkg).or_insert(BareSpecifierInfo {
                type_only: true,
                importers: Vec::new(),
            });
            if !imp.is_type_only {
                entry.type_only = false;
            }
            if is_new_importer {
                entry.importers.push(file.clone());
            }
        }
    }
    result
}

/// `@scope/pkg/deep/path` → `@scope/pkg`, `lodash/fp` → `lodash`.
fn extract_package_name(specifier: &str) -> String {
    if specifier.starts_with('@') {
        let parts: Vec<&str> = specifier.splitn(3, '/').collect();
        if parts.len() >= 2 && !parts[1].is_empty() {
            return format!("{}/{}", parts[0], parts[1]);
        }
        return String::new();
    }
    specifier.split('/').next().unwrap_or("").to_string()
}

fn is_builtin_module(name: &str) -> bool {
    // Cloudflare Workers runtime modules live under the `cloudflare:` protocol
    // namespace (e.g. `cloudflare:workers`, `cloudflare:sockets`). They are
    // runtime-provided built-ins, never npm packages, so the submodule name is
    // not enumerated.
    if name.starts_with("cloudflare:") {
        return true;
    }
    // Node.js built-in modules — bare imports that aren't npm packages.
    let name = name.strip_prefix("node:").unwrap_or(name);
    matches!(
        name,
        "assert"
            | "async_hooks"
            | "buffer"
            | "child_process"
            | "cluster"
            | "console"
            | "constants"
            | "crypto"
            | "dgram"
            | "diagnostics_channel"
            | "dns"
            | "domain"
            | "events"
            | "fs"
            | "http"
            | "http2"
            | "https"
            | "inspector"
            | "module"
            | "net"
            | "os"
            | "path"
            | "perf_hooks"
            | "process"
            | "punycode"
            | "querystring"
            | "readline"
            | "repl"
            | "stream"
            | "string_decoder"
            | "sys"
            | "test"
            | "timers"
            | "tls"
            | "trace_events"
            | "tty"
            | "url"
            | "util"
            | "v8"
            | "vm"
            | "wasi"
            | "worker_threads"
            | "zlib"
    )
}

fn is_indexable(lang: Language) -> bool {
    matches!(
        lang,
        Language::TypeScript
            | Language::Tsx
            | Language::JavaScript
            | Language::Rust
            | Language::Vue
            | Language::Markdown
    )
}

/// Iterative Tarjan SCC — returns only components with 2+ members (cycles).
/// Type-only edges are excluded so `import type` doesn't create false cycles.
fn compute_cycles(imports: &HashMap<PathBuf, Vec<ImportedSymbol>>) -> Vec<Vec<PathBuf>> {
    let mut adj: HashMap<&Path, Vec<&Path>> = HashMap::new();
    let mut all_nodes: HashSet<&Path> = HashSet::new();
    for (file, imps) in imports {
        all_nodes.insert(file.as_path());
        for imp in imps {
            if imp.is_type_only {
                continue;
            }
            if let Some(src) = &imp.source_path {
                adj.entry(file.as_path()).or_default().push(src.as_path());
                all_nodes.insert(src.as_path());
            }
        }
    }

    let mut index_counter: u32 = 0;
    let mut indices: HashMap<&Path, u32> = HashMap::new();
    let mut lowlinks: HashMap<&Path, u32> = HashMap::new();
    let mut on_stack: HashSet<&Path> = HashSet::new();
    let mut stack: Vec<&Path> = Vec::new();
    let mut result: Vec<Vec<PathBuf>> = Vec::new();

    for &root in &all_nodes {
        if indices.contains_key(root) {
            continue;
        }

        indices.insert(root, index_counter);
        lowlinks.insert(root, index_counter);
        index_counter += 1;
        on_stack.insert(root);
        stack.push(root);

        let mut dfs: Vec<(&Path, usize)> = vec![(root, 0)];

        while let Some(&(v, i)) = dfs.last() {
            let neighbors = adj.get(v).map_or(&[][..], Vec::as_slice);
            if i < neighbors.len() {
                let w = neighbors[i];
                dfs.last_mut().unwrap().1 = i + 1;
                if !indices.contains_key(w) {
                    indices.insert(w, index_counter);
                    lowlinks.insert(w, index_counter);
                    index_counter += 1;
                    on_stack.insert(w);
                    stack.push(w);
                    dfs.push((w, 0));
                } else if on_stack.contains(w) {
                    let cur = lowlinks[v];
                    let w_idx = indices[w];
                    lowlinks.insert(v, cur.min(w_idx));
                }
            } else {
                if lowlinks[v] == indices[v] {
                    let mut scc = Vec::new();
                    loop {
                        let w = stack.pop().expect("tarjan stack non-empty at scc root");
                        on_stack.remove(w);
                        scc.push(w.to_path_buf());
                        if w == v {
                            break;
                        }
                    }
                    if scc.len() > 1 {
                        result.push(scc);
                    }
                }
                let v_low = lowlinks[v];
                dfs.pop();
                if let Some(&(parent, _)) = dfs.last() {
                    let p_low = lowlinks[parent];
                    lowlinks.insert(parent, p_low.min(v_low));
                }
            }
        }
    }

    result
}

/// Raw per-file extract before cross-file resolution.
#[derive(Debug, Default, PartialEq, Eq)]
struct FileExtract {
    exports: Vec<ExportedSymbol>,
    imports: Vec<ImportedSymbol>,
    /// Raw call/new sites keyed by the local identifier at the call site.
    /// Cross-file linkage (local → exported name + source path) happens in
    /// `ImportIndex::build` using the file's import list.
    calls: Vec<LocalCall>,
    /// Relative directory prefixes of template-literal dynamic imports
    /// (`import(\`./locales/${lang}\`)` → `./locales`). The runtime path is
    /// computed, so it cannot resolve to one file; resolution treats the whole
    /// directory as referenced. Resolved to absolute dirs in `ImportIndex::build`.
    dynamic_dirs: Vec<String>,
}

/// A `new X(...)` / `X(...)` site captured during per-file extract. The
/// `local_name` is the identifier as written in this file; it is linked to an
/// exporting file + exported name later via the import list.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalCall {
    local_name: String,
    line: usize,
    column: usize,
    byte_offset: usize,
    byte_len: usize,
    kind: CallKind,
    args: Vec<Option<String>>,
}

fn extract_for(parser: &mut Parser, file: &SourceFile) -> Option<(PathBuf, FileExtract)> {
    let source = std::fs::read_to_string(&file.path).ok()?;
    if matches!(file.language, Language::Rust) {
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&lang).ok()?;
        let tree = parser.parse(source.as_bytes(), None)?;
        let (exports, imports) = extract_rust(&tree, source.as_bytes());
        let canon = std::fs::canonicalize(&file.path).unwrap_or_else(|_| file.path.clone());
        // Cross-file call-site tracking is TS-only for now.
        return Some((
            canon,
            FileExtract {
                exports,
                imports,
                calls: Vec::new(),
                dynamic_dirs: Vec::new(),
            },
        ));
    }
    if matches!(file.language, Language::Vue) {
        return extract_vue(parser, &source, &file.path);
    }
    if matches!(file.language, Language::Markdown) {
        return extract_markdown(&source, &file.path);
    }
    if !matches!(
        file.language,
        Language::Tsx | Language::TypeScript | Language::JavaScript
    ) {
        return None;
    }

    // Extract imports/exports/calls from oxc's AST — the same fast parser the
    // engine already uses — instead of a second, slower tree-sitter parse.
    // Wrapped in `catch_unwind` because the oxc parser can panic on
    // pathological input; a failed parse drops the file from the index, the
    // same outcome a tree-sitter parse failure produced.
    let extract = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_ts_oxc(&source, &file.path)
    }))
    .ok()
    .flatten()?;

    // Absolute-path canonicalization: rules compare paths by value, so two
    // different spellings of the same file (relative vs absolute) would miss
    // each other. Fall back to the given path if canonicalize fails.
    let canon = std::fs::canonicalize(&file.path).unwrap_or_else(|_| file.path.clone());
    Some((canon, extract))
}

fn extract_vue(parser: &mut Parser, source: &str, path: &Path) -> Option<(PathBuf, FileExtract)> {
    let vue_lang = tree_sitter_vue_updated::language();
    parser.set_language(&vue_lang).ok()?;
    let vue_tree = parser.parse(source.as_bytes(), None)?;
    let blocks = crate::rules::vue_sfc::extract_scripts(&vue_tree, source);

    let ts_grammar: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let mut exports = Vec::new();
    let mut imports = Vec::new();
    let mut calls = Vec::new();
    let mut dynamic_dirs = Vec::new();
    let mut has_setup = false;

    for block in &blocks {
        if block.is_setup {
            has_setup = true;
        }
        parser.set_language(&ts_grammar).ok()?;
        let Some(tree) = parser.parse(block.text.as_bytes(), None) else {
            continue;
        };
        let row_offset = block.start_row;
        let source_bytes = block.text.as_bytes();
        let imp_start = imports.len();
        let exp_start = exports.len();
        let call_start = calls.len();
        walk_tree(&tree, |node| match node.kind() {
            "import_statement" => extract_import(node, source_bytes, &mut imports),
            "export_statement" => extract_export(node, source_bytes, &mut exports),
            "new_expression" => {
                extract_call(node, source_bytes, CallKind::New, &mut calls);
            }
            "call_expression" => {
                if node
                    .child_by_field_name("function")
                    .is_some_and(|c| c.kind() == "import")
                {
                    extract_dynamic_import(node, source_bytes, &mut imports, &mut dynamic_dirs);
                } else {
                    extract_require(node, source_bytes, &mut imports);
                    extract_call(node, source_bytes, CallKind::Call, &mut calls);
                }
            }
            _ => {}
        });
        for imp in &mut imports[imp_start..] {
            imp.line += row_offset;
        }
        for exp in &mut exports[exp_start..] {
            exp.line += row_offset;
        }
        for call in &mut calls[call_start..] {
            call.line += row_offset;
        }
    }

    // Every Vue SFC implicitly has a default export (the component).
    if has_setup || !exports.iter().any(|e| e.kind == ExportKind::Default) {
        exports.push(ExportedSymbol {
            name: "default".to_string(),
            kind: ExportKind::Default,
            line: 1,
            reexport_source: None,
            params: Vec::new(),
            is_type_only: false,
            local_name: None,
        });
    }

    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Some((
        canon,
        FileExtract {
            exports,
            imports,
            calls,
            dynamic_dirs,
        },
    ))
}

/// Extract the top-of-file ESM `import` statements from a Markdown / MDX file.
///
/// MDX (and MDX-flavored Markdown processed by Docusaurus / Nextra / Astro) uses
/// standard ESM `import … from '…'` statements, so a component consumed only
/// from a docs page is a real cross-file usage. Markdown is not valid JS, so the
/// whole file is never handed to the parser: import-statement line spans are
/// isolated (skipping fenced code blocks and prose), every other line is blanked
/// to a newline to preserve line numbers, and the resulting import-only program
/// is parsed by oxc — reusing the exact import-clause logic as TS/JS imports.
///
/// Only import edges are produced; a Markdown file declares no exports.
fn extract_markdown(source: &str, path: &Path) -> Option<(PathBuf, FileExtract)> {
    let import_only = blank_non_import_lines(source);
    if !import_only.contains("import") {
        // No ESM imports — index the file as a participant with no edges so it
        // still counts toward the file set, mirroring an import-free TS module.
        let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        return Some((canon, FileExtract::default()));
    }

    let imports = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_imports_from_module(&import_only)
    }))
    .ok()
    .unwrap_or_default();

    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Some((
        canon,
        FileExtract {
            imports,
            ..FileExtract::default()
        },
    ))
}

/// Parse `source` as a JS module and return only its import edges. Shares the
/// oxc import-clause extraction used for real TS/JS files so named/default/
/// namespace/type-only/aliased imports are recorded identically.
fn extract_imports_from_module(source: &str) -> Vec<ImportedSymbol> {
    use oxc_allocator::Allocator;
    use oxc_ast::AstKind;
    use oxc_parser::Parser as OxcParser;
    use oxc_span::SourceType;

    let allocator = Allocator::default();
    let parse_ret = OxcParser::new(&allocator, source, SourceType::mjs()).parse();
    let semantic = oxc_semantic::SemanticBuilder::new()
        .build(&parse_ret.program)
        .semantic;

    let lines = oxc_line_starts(source);
    let mut imports = Vec::new();
    for node in semantic.nodes().iter() {
        if let AstKind::ImportDeclaration(import) = node.kind() {
            oxc_extract_import(&lines, import, &mut imports);
        }
    }
    imports
}

/// Reduce a Markdown / MDX file to its ESM import statements, blanking every
/// other line to an empty line so import line numbers are preserved and the
/// result parses as JS. Lines inside fenced code blocks (```` ``` ```` / `~~~`)
/// are treated as prose. A statement starts at a line whose first non-blank
/// token is the `import` keyword and continues until the line that closes the
/// `from '…'` / bare `import '…'` specifier with a string quote — so multi-line
/// import clauses survive while inline `import` words in prose are dropped.
fn blank_non_import_lines(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_fence = false;
    let mut in_import = false;
    for line in source.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let trimmed = body.trim_start();

        if !in_import && is_code_fence(trimmed) {
            in_fence = !in_fence;
            out.push('\n');
            continue;
        }

        let keep = if in_import {
            true
        } else {
            !in_fence && starts_import_statement(trimmed)
        };

        if keep {
            out.push_str(body);
            in_import = !import_statement_complete(body);
        }
        out.push('\n');
    }
    out
}

/// True when `trimmed` opens or closes a fenced code block (```` ``` ```` or
/// `~~~`, optionally with an info string).
fn is_code_fence(trimmed: &str) -> bool {
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// True when `trimmed` begins with the `import` keyword as a statement — the
/// next character is whitespace or one of the clause openers `{ * ' " (`. This
/// rejects identifiers like `imported` and prose sentences containing `import`
/// mid-line (those never begin the line with the bare keyword).
fn starts_import_statement(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix("import") else {
        return false;
    };
    match rest.chars().next() {
        None => true,
        Some(c) => c.is_whitespace() || matches!(c, '{' | '*' | '\'' | '"' | '('),
    }
}

/// True when an import statement that began on a prior line is closed on `body`
/// — i.e. the line contains a string-quote terminator for the specifier. A
/// single-line `import … from '…'` is complete on its own line; a multi-line
/// clause stays open until the closing line carries the quoted specifier.
fn import_statement_complete(body: &str) -> bool {
    body.contains('\'') || body.contains('"') || body.contains('`')
}

/// Capture a `new_expression` / `call_expression` when its callee is a bare
/// identifier. Member-access calls (`foo.bar()`, `ns.Foo()`) and computed
/// callees are skipped — this index only tracks direct references to imported
/// names, not property access on namespaces.
fn extract_call(node: Node, source: &[u8], kind: CallKind, out: &mut Vec<LocalCall>) {
    let field = match kind {
        CallKind::New => "constructor",
        CallKind::Call => "function",
    };
    let Some(callee) = node.child_by_field_name(field) else {
        return;
    };
    if callee.kind() != "identifier" {
        return;
    }
    let Ok(name) = callee.utf8_text(source) else {
        return;
    };
    let pos = node.start_position();
    let range = node.byte_range();

    // Extract argument names (None for non-identifier arguments)
    let mut args = Vec::new();
    if let Some(args_node) = node.child_by_field_name("arguments") {
        let mut cursor = args_node.walk();
        for child in args_node.named_children(&mut cursor) {
            if child.kind() == "identifier" {
                args.push(child.utf8_text(source).ok().map(String::from));
            } else {
                args.push(None);
            }
        }
    }

    out.push(LocalCall {
        local_name: name.to_string(),
        line: pos.row + 1,
        column: pos.column + 1,
        byte_offset: range.start,
        byte_len: range.len(),
        kind,
        args,
    });
}

fn extract_import(node: Node, source: &[u8], out: &mut Vec<ImportedSymbol>) {
    let Some(specifier) = find_specifier_string(node, source) else {
        return;
    };
    let line = node.start_position().row + 1;

    // `import type { X }` — statement-level type-only: the second child
    // (index 1, right after `import`) is the `type` keyword.
    let stmt_type_only = node.child(1).is_some_and(|c| c.kind() == "type");

    // Find the import clause child — if absent, it's a side-effect import.
    let clause = node
        .named_children(&mut node.walk())
        .find(|c| c.kind() == "import_clause");
    let Some(clause) = clause else {
        out.push(ImportedSymbol {
            local_name: String::new(),
            imported_name: String::new(),
            kind: ImportKind::SideEffect,
            specifier,
            source_path: None,
            line,
            is_type_only: false,
        });
        return;
    };

    let mut cursor = clause.walk();
    for child in clause.named_children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                // `import foo from '…'`
                let name = text_of(child, source);
                out.push(ImportedSymbol {
                    local_name: name.clone(),
                    imported_name: "default".into(),
                    kind: ImportKind::Default,
                    specifier: specifier.clone(),
                    source_path: None,
                    line,
                    is_type_only: stmt_type_only,
                });
            }
            "namespace_import" => {
                // `import * as foo from '…'` — single identifier child.
                if let Some(id) = child
                    .named_children(&mut child.walk())
                    .find(|c| c.kind() == "identifier")
                {
                    let name = text_of(id, source);
                    out.push(ImportedSymbol {
                        local_name: name,
                        imported_name: "*".into(),
                        kind: ImportKind::Namespace,
                        specifier: specifier.clone(),
                        source_path: None,
                        line,
                        is_type_only: stmt_type_only,
                    });
                }
            }
            "named_imports" => {
                let mut nested = child.walk();
                for spec in child.named_children(&mut nested) {
                    if spec.kind() != "import_specifier" {
                        continue;
                    }
                    // Per-specifier `type`: `import { type X }`.
                    let spec_type_only = stmt_type_only
                        || spec.children(&mut spec.walk()).any(|c| c.kind() == "type");
                    let (imported, local) = import_specifier_names(spec, source);
                    out.push(ImportedSymbol {
                        local_name: local,
                        imported_name: imported,
                        kind: ImportKind::Named,
                        specifier: specifier.clone(),
                        source_path: None,
                        line,
                        is_type_only: spec_type_only,
                    });
                }
            }
            _ => {}
        }
    }
}

/// Capture `import('./foo')` as a namespace-style import edge.
fn extract_dynamic_import(
    node: Node,
    source: &[u8],
    out: &mut Vec<ImportedSymbol>,
    dyn_dirs: &mut Vec<String>,
) {
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let mut cursor = args.walk();
    let Some(first_arg) = args.named_children(&mut cursor).next() else {
        return;
    };
    // `import(\`./locales/${lang}\`)` — the path is computed at runtime, so it
    // names a directory rather than one file. Record the static directory prefix.
    if first_arg.kind() == "template_string" {
        let has_substitution = first_arg
            .children(&mut first_arg.walk())
            .any(|c| c.kind() == "template_substitution");
        if has_substitution {
            let raw = text_of(first_arg, source);
            let inner = raw.trim_start_matches('`');
            let static_prefix = inner.split("${").next().unwrap_or("");
            if let Some(dir) = template_static_dir(static_prefix) {
                dyn_dirs.push(dir);
            }
        }
        return;
    }
    if first_arg.kind() != "string" {
        return;
    }
    let raw = text_of(first_arg, source);
    let specifier = raw.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    if specifier.is_empty() {
        return;
    }
    out.push(ImportedSymbol {
        local_name: String::new(),
        imported_name: "*".into(),
        kind: ImportKind::Namespace,
        specifier: specifier.to_string(),
        source_path: None,
        line: node.start_position().row + 1,
        is_type_only: false,
    });
}

/// Static directory prefix of an interpolated template-literal import path.
///
/// `static_prefix` is the literal text that precedes the first `${…}`
/// substitution (`../a/b/${x}` → `../a/b/`, `./locales/${lang}.ts` →
/// `./locales/`). Returns the directory of that prefix: `../a/b/` → `../a/b`,
/// `./locales/` → `./locales`. Returns `None` when the prefix is not relative
/// (a bare-package or alias specifier, whose target directory is not in the
/// input set) or when no directory separator precedes the substitution
/// (`./${name}` — the whole importer directory, too broad to mark referenced).
fn template_static_dir(static_prefix: &str) -> Option<String> {
    if !static_prefix.starts_with('.') {
        return None;
    }
    let dir = static_prefix.rsplit_once('/')?.0;
    if dir.is_empty() {
        return None;
    }
    Some(dir.to_string())
}

/// Capture `require('./foo')` as a namespace-style import edge.
fn extract_require(node: Node, source: &[u8], out: &mut Vec<ImportedSymbol>) {
    let Some(callee) = node.child_by_field_name("function") else {
        return;
    };
    if callee.kind() != "identifier" || text_of(callee, source) != "require" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let mut cursor = args.walk();
    let Some(first_arg) = args.named_children(&mut cursor).next() else {
        return;
    };
    if first_arg.kind() != "string" {
        return;
    }
    let raw = text_of(first_arg, source);
    let specifier = raw.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    if specifier.is_empty() {
        return;
    }
    out.push(ImportedSymbol {
        local_name: String::new(),
        imported_name: "*".into(),
        kind: ImportKind::Namespace,
        specifier: specifier.to_string(),
        source_path: None,
        line: node.start_position().row + 1,
        is_type_only: false,
    });
}

/// Extract (imported_name, local_name) from an `import_specifier`. Handles
/// both `{ a }` (both names equal) and `{ a as b }` (imported=a, local=b).
fn import_specifier_names(spec: Node, source: &[u8]) -> (String, String) {
    // tree-sitter-typescript names the children `name` and `alias` via fields,
    // but we can't rely on field_name availability across grammar versions —
    // fall back to positional: first identifier = imported, second = local.
    let ids: Vec<Node> = spec
        .named_children(&mut spec.walk())
        .filter(|c| c.kind() == "identifier")
        .collect();
    match ids.as_slice() {
        [single] => {
            let n = text_of(*single, source);
            (n.clone(), n)
        }
        [imported, local, ..] => (text_of(*imported, source), text_of(*local, source)),
        [] => (String::new(), String::new()),
    }
}

fn extract_export(node: Node, source: &[u8], out: &mut Vec<ExportedSymbol>) {
    // Exports nested inside a `namespace Foo { … }` / `module Foo { … }` body are
    // scoped to that namespace — reachable only as `Foo.X`, never as a
    // module-level binding. The namespace itself is recorded as the module-level
    // export by its enclosing `export_statement`; its members are skipped, so two
    // namespaces may each export an `X` without it being a module-level duplicate.
    if is_in_ts_namespace_body(node) {
        return;
    }

    let line = node.start_position().row + 1;

    // `export * from './m'` is a bare `*` token child of export_statement.
    // `export * as ns from './m'` nests the `*` inside a `namespace_export`
    // node, so the wildcard is detected via either shape.
    let has_star = node
        .children(&mut node.walk())
        .any(|c| c.kind() == "*" || c.kind() == "namespace_export");
    let source_str = find_specifier_string(node, source);

    if let Some(src) = &source_str
        && has_star
    {
        // Distinguish `export * as ns from '…'` (named re-export under `ns`)
        // from bare `export * from '…'`.
        if let Some(ns) = node
            .named_children(&mut node.walk())
            .find(|c| c.kind() == "namespace_export")
            && let Some(id) = ns
                .named_children(&mut ns.walk())
                .find(|c| c.kind() == "identifier")
        {
            out.push(ExportedSymbol {
                name: text_of(id, source),
                kind: ExportKind::ReExport,
                line,
                reexport_source: Some(src.clone()),
                params: Vec::new(),
                is_type_only: false,
                local_name: None,
            });
            return;
        }
        out.push(ExportedSymbol {
            name: "*".into(),
            kind: ExportKind::StarReExport,
            line,
            reexport_source: Some(src.clone()),
            params: Vec::new(),
            is_type_only: false,
            local_name: None,
        });
        return;
    }

    // `export { a, b as c } [from '…']`
    if let Some(clause) = node
        .named_children(&mut node.walk())
        .find(|c| c.kind() == "export_clause")
    {
        let kind = if source_str.is_some() {
            ExportKind::ReExport
        } else {
            ExportKind::Named
        };
        let mut cursor = clause.walk();
        for spec in clause.named_children(&mut cursor) {
            if spec.kind() != "export_specifier" {
                continue;
            }
            // Same positional logic as import_specifier: first ident = local
            // export source, second (if present) = exported name. For
            // re-exports, the exported name is what outside callers see.
            let ids: Vec<Node> = spec
                .named_children(&mut spec.walk())
                .filter(|c| c.kind() == "identifier")
                .collect();
            let (name, local_name) = match ids.as_slice() {
                [single] => (text_of(*single, source), None),
                [local, aliased, ..] => {
                    (text_of(*aliased, source), Some(text_of(*local, source)))
                }
                [] => continue,
            };
            out.push(ExportedSymbol {
                name,
                kind,
                line,
                reexport_source: source_str.clone(),
                params: Vec::new(),
                is_type_only: false,
                local_name,
            });
        }
        return;
    }

    // `export default …`
    let is_default = node
        .children(&mut node.walk())
        .any(|c| c.kind() == "default");
    if is_default {
        out.push(ExportedSymbol {
            name: "default".into(),
            kind: ExportKind::Default,
            line,
            reexport_source: None,
            params: Vec::new(),
            is_type_only: false,
            local_name: None,
        });
        return;
    }

    // `export function foo` / `export class Foo` / `export const foo = …` /
    // `export type Foo = …` / `export interface Foo` / `export enum Foo`.
    // Ambient `export declare …` wraps its declaration in an
    // `ambient_declaration` node; recurse into it so the body-less ambient
    // forms (`export declare function/const/class/namespace/enum`) are indexed
    // as real named exports.
    for child in node.named_children(&mut node.walk()) {
        if child.kind() == "ambient_declaration" {
            // The inner declaration of `export declare …` is a real named
            // export. Its body-less `function_signature` is an ambient
            // function (not an overload), so it must be indexed here — unlike a
            // bare `function_signature` directly under `export_statement`,
            // which is an overload signature and is skipped below.
            if let Some(inner) = child
                .named_children(&mut child.walk())
                .find(|c| c.kind() != "comment")
            {
                if inner.kind() == "function_signature" {
                    if let Some(id) = inner
                        .named_children(&mut inner.walk())
                        .find(|c| c.kind() == "identifier")
                    {
                        out.push(ExportedSymbol {
                            name: text_of(id, source),
                            kind: ExportKind::Named,
                            line,
                            reexport_source: None,
                            params: extract_params(inner, source),
                            is_type_only: false,
                            local_name: None,
                        });
                    }
                } else {
                    extract_declaration_export(inner, source, line, out);
                }
            }
            continue;
        }
        extract_declaration_export(child, source, line, out);
    }
}

/// True when `node` sits inside a TypeScript namespace/module body — some
/// ancestor is an `internal_module` (`namespace Foo`) or `module` (`module Foo`)
/// node. tree-sitter analogue of `oxc_helpers::is_in_ts_namespace`.
fn is_in_ts_namespace_body(node: Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if matches!(parent.kind(), "internal_module" | "module") {
            return true;
        }
        current = parent.parent();
    }
    false
}

/// Extract exported name(s) from a declaration node directly under an
/// `export_statement`. A bare `function_signature` (overload signature) is
/// deliberately not matched, so an overloaded `export function` yields one
/// export, not two; ambient `export declare function` is handled separately by
/// the caller.
fn extract_declaration_export(child: Node, source: &[u8], line: usize, out: &mut Vec<ExportedSymbol>) {
    match child.kind() {
        "function_declaration" | "generator_function_declaration" => {
            if let Some(id) = child
                .named_children(&mut child.walk())
                .find(|c| c.kind() == "identifier")
            {
                let params = extract_params(child, source);
                out.push(ExportedSymbol {
                    name: text_of(id, source),
                    kind: ExportKind::Named,
                    line,
                    reexport_source: None,
                    params,
                    is_type_only: false,
                    local_name: None,
                });
            }
        }
        "class_declaration" | "abstract_class_declaration" => {
            if let Some(id) = child
                .named_children(&mut child.walk())
                .find(|c| c.kind() == "identifier" || c.kind() == "type_identifier")
            {
                out.push(ExportedSymbol {
                    name: text_of(id, source),
                    kind: ExportKind::Named,
                    line,
                    reexport_source: None,
                    params: Vec::new(),
                    is_type_only: false,
                    local_name: None,
                });
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            // `const a = 1, b = 2` can export multiple names, and the
            // LHS may also be a destructuring pattern:
            // `export const { signIn, signOut } = authClient` —
            // every property identifier in the pattern is a real
            // export. Same for array patterns and nested ones.
            let mut inner = child.walk();
            for decl in child.named_children(&mut inner) {
                if decl.kind() != "variable_declarator" {
                    continue;
                }
                let Some(name_node) = decl
                    .named_children(&mut decl.walk())
                    .find(|c| is_binding_pattern_kind(c.kind()))
                else {
                    continue;
                };
                let mut names = Vec::new();
                collect_pattern_names(name_node, source, &mut names);
                for name in names {
                    out.push(ExportedSymbol {
                        name,
                        kind: ExportKind::Named,
                        line,
                        reexport_source: None,
                        params: Vec::new(),
                        is_type_only: false,
                        local_name: None,
                    });
                }
            }
        }
        "type_alias_declaration" | "interface_declaration" => {
            if let Some(id) = child
                .named_children(&mut child.walk())
                .find(|c| c.kind() == "type_identifier" || c.kind() == "identifier")
            {
                out.push(ExportedSymbol {
                    name: text_of(id, source),
                    kind: ExportKind::Named,
                    line,
                    reexport_source: None,
                    params: Vec::new(),
                    is_type_only: true,
                    local_name: None,
                });
            }
        }
        "enum_declaration" => {
            if let Some(id) = child
                .named_children(&mut child.walk())
                .find(|c| c.kind() == "type_identifier" || c.kind() == "identifier")
            {
                out.push(ExportedSymbol {
                    name: text_of(id, source),
                    kind: ExportKind::Named,
                    line,
                    reexport_source: None,
                    params: Vec::new(),
                    is_type_only: false,
                    local_name: None,
                });
            }
        }
        "internal_module" | "module" => {
            // `export [declare] namespace Foo { … }` / `module Foo { … }`.
            if let Some(id) = child
                .named_children(&mut child.walk())
                .find(|c| c.kind() == "identifier")
            {
                out.push(ExportedSymbol {
                    name: text_of(id, source),
                    kind: ExportKind::Named,
                    line,
                    reexport_source: None,
                    params: Vec::new(),
                    is_type_only: false,
                    local_name: None,
                });
            }
        }
        _ => {}
    }
}

/// Find the `string` child of an import/export statement — the module
/// specifier. Returns the unquoted contents.
fn find_specifier_string(node: Node, source: &[u8]) -> Option<String> {
    let str_node = node
        .named_children(&mut node.walk())
        .find(|c| c.kind() == "string")?;
    let raw = text_of(str_node, source);
    Some(
        raw.trim_matches(|c| c == '\'' || c == '"' || c == '`')
            .to_string(),
    )
}

fn text_of(node: Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

/// Tree-sitter node kinds that can sit on the LHS of a
/// `variable_declarator`: a single name, or an object / array pattern.
fn is_binding_pattern_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier" | "object_pattern" | "array_pattern" | "rest_pattern"
    )
}

/// Walk a destructuring binding pattern and push every introduced
/// identifier name into `out`. Handles object, array, nested, default,
/// and rest patterns:
///
/// - `{ a, b }`                → `a`, `b`
/// - `{ a: renamedA }`         → `renamedA`
/// - `{ a = 1 }`               → `a`
/// - `[a, b, ...rest]`         → `a`, `b`, `rest`
/// - `{ a: { b }, ...rest }`   → `b`, `rest`
fn collect_pattern_names(node: Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "identifier" => out.push(text_of(node, source)),
        "shorthand_property_identifier_pattern" => out.push(text_of(node, source)),
        "object_pattern" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_pattern_names(child, source, out);
            }
        }
        "array_pattern" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_pattern_names(child, source, out);
            }
        }
        "pair_pattern" => {
            // `{ a: localName }` — the value side is what's actually bound.
            if let Some(value) = node.child_by_field_name("value") {
                collect_pattern_names(value, source, out);
            }
        }
        "assignment_pattern" | "object_assignment_pattern" => {
            // `{ a = 1 }` / `[a = 1]` — the left side is the binding.
            if let Some(left) = node.child_by_field_name("left") {
                collect_pattern_names(left, source, out);
            }
        }
        "rest_pattern" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_pattern_names(child, source, out);
            }
        }
        _ => {}
    }
}

/// Extract parameter names from a function declaration node.
fn extract_params(node: Node, source: &[u8]) -> Vec<String> {
    let Some(params) = node.child_by_field_name("parameters") else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut cursor = params.walk();
    for child in params.named_children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                result.push(text_of(child, source));
            }
            "required_parameter" | "optional_parameter" => {
                if let Some(id) = child.child_by_field_name("pattern")
                    && id.kind() == "identifier"
                {
                    result.push(text_of(id, source));
                }
            }
            _ => {}
        }
    }
    result
}

// ===========================================================================
// oxc-based extraction (TS/JS/TSX) — byte-exact equivalent of the tree-sitter
// path above. Replicates `extract_for`'s TS dispatch and every helper it calls
// (`extract_import` / `extract_export` / `extract_call` / `extract_dynamic_import`
// / `extract_require`) using the typed oxc AST. The Rust and Vue paths stay on
// tree-sitter. Not yet wired into `build()`; validated by the differential test
// `oxc_matches_treesitter` below.
// ===========================================================================

/// 1-based line of `offset` = 1 + count of `\n` bytes in `source[..offset]`.
/// Matches tree-sitter's `start_position().row + 1`.
/// Byte offsets of every line start (index 0 + the byte after each `\n`).
/// Built once per file so line/column lookups are O(log lines) binary searches
/// instead of an O(offset) rescan per import/export/call node.
fn oxc_line_starts(source: &str) -> Vec<usize> {
    let mut starts = Vec::with_capacity(source.len() / 32 + 1);
    starts.push(0);
    starts.extend(
        source
            .bytes()
            .enumerate()
            .filter(|(_, b)| *b == b'\n')
            .map(|(i, _)| i + 1),
    );
    starts
}

/// 1-based line number of `offset`, matching tree-sitter's
/// `start_position().row + 1`.
fn oxc_line_at(lines: &[usize], offset: usize) -> usize {
    lines.partition_point(|&start| start <= offset)
}

/// 1-based column of `offset` as a BYTE offset from the last `\n` before it,
/// matching tree-sitter's `start_position().column + 1` (tree-sitter columns
/// are byte offsets, not char counts).
fn oxc_column_at(lines: &[usize], offset: usize) -> usize {
    let line = lines.partition_point(|&start| start <= offset);
    (offset - lines[line - 1]) + 1
}

/// oxc equivalent of `extract_for`'s TS/JS/TSX branch. Returns the same
/// `FileExtract` (same elements, same order) as the tree-sitter path.
/// `source` is the file text; `path` selects the oxc `SourceType`.
fn extract_ts_oxc(source: &str, path: &Path) -> Option<FileExtract> {
    use oxc_allocator::Allocator;
    use oxc_ast::AstKind;
    use oxc_parser::Parser as OxcParser;

    let source_type = crate::oxc_helpers::source_type_for_path(path);
    let allocator = Allocator::default();
    let parse_ret = OxcParser::new(&allocator, source, source_type).parse();
    let semantic = oxc_semantic::SemanticBuilder::new()
        .build(&parse_ret.program)
        .semantic;

    let mut exports = Vec::new();
    let mut imports = Vec::new();
    let mut calls = Vec::new();
    let mut dynamic_dirs = Vec::new();
    let lines = oxc_line_starts(source);

    // Pre-order over `nodes().iter()` (NodeId order == SemanticBuilder visit
    // order == pre-order DFS), the same traversal `walk_tree` performs.
    for node in semantic.nodes().iter() {
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                oxc_extract_import(&lines, import, &mut imports);
            }
            // Exports nested inside a `declare module '...'` / `declare global`
            // block are TypeScript module augmentations: the compiler merges them
            // into another module's types, so they are never imported by name and
            // must not be counted as project exports (else dead-export flags them).
            //
            // Exports nested inside a `namespace Foo { … }` / `module Foo { … }`
            // body are scoped to that namespace: `export interface X` there is
            // reachable only as `Foo.X`, never as a module-level binding. The
            // namespace itself (`export namespace Foo`) is recorded as the
            // module-level export; its members are not, so two namespaces may each
            // export an `X` without it being a module-level duplicate.
            AstKind::ExportNamedDeclaration(export)
                if !crate::oxc_helpers::is_in_ambient_declaration(node.id(), &semantic)
                    && !crate::oxc_helpers::is_in_ts_namespace(node.id(), &semantic) =>
            {
                oxc_extract_export_named(&lines, export, &mut exports);
            }
            AstKind::ExportAllDeclaration(export)
                if !crate::oxc_helpers::is_in_ambient_declaration(node.id(), &semantic)
                    && !crate::oxc_helpers::is_in_ts_namespace(node.id(), &semantic) =>
            {
                oxc_extract_export_all(&lines, export, &mut exports);
            }
            AstKind::ExportDefaultDeclaration(export)
                if !crate::oxc_helpers::is_in_ambient_declaration(node.id(), &semantic)
                    && !crate::oxc_helpers::is_in_ts_namespace(node.id(), &semantic) =>
            {
                exports.push(ExportedSymbol {
                    name: "default".into(),
                    kind: ExportKind::Default,
                    line: oxc_line_at(&lines, export.span.start as usize),
                    reexport_source: None,
                    params: Vec::new(),
                    is_type_only: false,
                    local_name: None,
                });
            }
            AstKind::NewExpression(new_expr) => {
                oxc_extract_call_new(&lines, new_expr, &mut calls);
            }
            AstKind::CallExpression(call) => {
                oxc_extract_require(&lines, call, &mut imports);
                oxc_extract_call_call(&lines, call, &mut calls);
            }
            AstKind::ImportExpression(import_expr) => {
                oxc_extract_dynamic_import(&lines, import_expr, &mut imports, &mut dynamic_dirs);
            }
            _ => {}
        }
    }

    Some(FileExtract {
        exports,
        imports,
        calls,
        dynamic_dirs,
    })
}

/// Named export names declared in a TypeScript declaration file (`.d.ts` and
/// variants). Reads and parses the file on demand — declaration files are
/// excluded from the indexed set, so their exports are not in `get_exports`.
///
/// Returns `None` when the declaration cannot be enumerated reliably: the file
/// is unreadable, fails to parse, or contains an `export * from '...'` star
/// re-export (which pulls in names this single-file parse cannot see). A `None`
/// result means "cannot prove the name is absent", so callers must not flag.
#[must_use]
pub fn declaration_file_exports(path: &Path) -> Option<HashSet<String>> {
    let source = std::fs::read_to_string(path).ok()?;
    let extract = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_ts_oxc(&source, path)
    }))
    .ok()
    .flatten()?;
    if extract
        .exports
        .iter()
        .any(|e| e.kind == ExportKind::StarReExport)
    {
        return None;
    }
    Some(extract.exports.into_iter().map(|e| e.name).collect())
}

fn oxc_extract_import(
    lines: &[usize],
    import: &oxc_ast::ast::ImportDeclaration,
    out: &mut Vec<ImportedSymbol>,
) {
    use oxc_ast::ast::ImportDeclarationSpecifier;

    let specifier = import.source.value.as_str().to_string();
    let line = oxc_line_at(lines, import.span.start as usize);
    let stmt_type_only = import.import_kind.is_type();

    // `import '...'` (side effect): `specifiers` is `None`. `import {} from '...'`
    // is `Some([])` and produces no symbols, like tree-sitter's empty clause.
    let Some(specifiers) = &import.specifiers else {
        out.push(ImportedSymbol {
            local_name: String::new(),
            imported_name: String::new(),
            kind: ImportKind::SideEffect,
            specifier,
            source_path: None,
            line,
            is_type_only: false,
        });
        return;
    };

    for spec in specifiers {
        match spec {
            ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => {
                out.push(ImportedSymbol {
                    local_name: def.local.name.as_str().to_string(),
                    imported_name: "default".into(),
                    kind: ImportKind::Default,
                    specifier: specifier.clone(),
                    source_path: None,
                    line,
                    is_type_only: stmt_type_only,
                });
            }
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => {
                out.push(ImportedSymbol {
                    local_name: ns.local.name.as_str().to_string(),
                    imported_name: "*".into(),
                    kind: ImportKind::Namespace,
                    specifier: specifier.clone(),
                    source_path: None,
                    line,
                    is_type_only: stmt_type_only,
                });
            }
            ImportDeclarationSpecifier::ImportSpecifier(named) => {
                let local = named.local.name.as_str().to_string();
                // tree-sitter only sees `identifier` nodes; `import { "x" as y }`
                // exposes a single identifier (`y`), so imported == local there.
                let imported = named
                    .imported
                    .identifier_name()
                    .map_or_else(|| local.clone(), |id| id.as_str().to_string());
                let spec_type_only = stmt_type_only || named.import_kind.is_type();
                out.push(ImportedSymbol {
                    local_name: local,
                    imported_name: imported,
                    kind: ImportKind::Named,
                    specifier: specifier.clone(),
                    source_path: None,
                    line,
                    is_type_only: spec_type_only,
                });
            }
        }
    }
}

fn oxc_extract_export_named(
    lines: &[usize],
    export: &oxc_ast::ast::ExportNamedDeclaration,
    out: &mut Vec<ExportedSymbol>,
) {
    use oxc_ast::ast::{Declaration, TSModuleDeclarationName};

    let line = oxc_line_at(lines, export.span.start as usize);
    let reexport_source = export.source.as_ref().map(|s| s.value.as_str().to_string());

    // `export { a, b as c } [from '...']`
    if export.declaration.is_none() {
        let kind = if reexport_source.is_some() {
            ExportKind::ReExport
        } else {
            ExportKind::Named
        };
        for spec in &export.specifiers {
            // tree-sitter positional logic: `{ a }` => one identifier (a);
            // `{ b as c }` => two identifiers, exported name = c. A string-literal
            // alias is not an identifier in tree-sitter, so it falls back to local.
            let local = spec.local.identifier_name().map(|id| id.as_str().to_string());
            // `export { x }` (no alias) has matching local/exported names; only
            // record `local_name` for `export { local as exported }`, matching
            // tree-sitter which exposes a single identifier in the unaliased case.
            let (name, local_name) = match spec.exported.identifier_name() {
                Some(exported) => {
                    let exported = exported.as_str().to_string();
                    let local_name = local.filter(|l| *l != exported);
                    (exported, local_name)
                }
                None => match local {
                    Some(id) => (id, None),
                    None => continue,
                },
            };
            out.push(ExportedSymbol {
                name,
                kind,
                line,
                reexport_source: reexport_source.clone(),
                params: Vec::new(),
                is_type_only: false,
                local_name,
            });
        }
        return;
    }

    // `export function foo` / `export class Foo` / `export const …` /
    // `export type/interface/enum …`. Ambient `export declare const/class …`
    // are real named exports (importable by name), so they are indexed too.
    match export.declaration.as_ref().unwrap() {
        Declaration::FunctionDeclaration(func) => {
            // An overload signature is body-less and not `declare` — it is
            // paired with an implementation in the same file, so it is skipped
            // to avoid double-counting. An ambient `export declare function` is
            // also body-less but IS the export (no implementation accompanies
            // it), so `declare` body-less functions are still indexed.
            if (func.body.is_some() || func.declare)
                && let Some(id) = &func.id
            {
                out.push(ExportedSymbol {
                    name: id.name.as_str().to_string(),
                    kind: ExportKind::Named,
                    line,
                    reexport_source: None,
                    params: oxc_extract_params(func),
                    is_type_only: false,
                    local_name: None,
                });
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                out.push(ExportedSymbol {
                    name: id.name.as_str().to_string(),
                    kind: ExportKind::Named,
                    line,
                    reexport_source: None,
                    params: Vec::new(),
                    is_type_only: false,
                    local_name: None,
                });
            }
        }
        Declaration::VariableDeclaration(var) => {
            for decl in &var.declarations {
                let mut names = Vec::new();
                oxc_collect_pattern_names(&decl.id, &mut names);
                for name in names {
                    out.push(ExportedSymbol {
                        name,
                        kind: ExportKind::Named,
                        line,
                        reexport_source: None,
                        params: Vec::new(),
                        is_type_only: false,
                        local_name: None,
                    });
                }
            }
        }
        Declaration::TSTypeAliasDeclaration(decl) => {
            out.push(ExportedSymbol {
                name: decl.id.name.as_str().to_string(),
                kind: ExportKind::Named,
                line,
                reexport_source: None,
                params: Vec::new(),
                is_type_only: true,
                local_name: None,
            });
        }
        Declaration::TSInterfaceDeclaration(decl) => {
            out.push(ExportedSymbol {
                name: decl.id.name.as_str().to_string(),
                kind: ExportKind::Named,
                line,
                reexport_source: None,
                params: Vec::new(),
                is_type_only: true,
                local_name: None,
            });
        }
        Declaration::TSEnumDeclaration(decl) => {
            out.push(ExportedSymbol {
                name: decl.id.name.as_str().to_string(),
                kind: ExportKind::Named,
                line,
                reexport_source: None,
                params: Vec::new(),
                is_type_only: false,
                local_name: None,
            });
        }
        Declaration::TSModuleDeclaration(decl) => {
            // `export [declare] namespace Foo { … }` / `module Foo { … }` — an
            // identifier-named namespace is importable by name. String-literal
            // module names (`declare module '*.css'`) are not importable
            // bindings, so they are skipped.
            if let TSModuleDeclarationName::Identifier(id) = &decl.id {
                out.push(ExportedSymbol {
                    name: id.name.as_str().to_string(),
                    kind: ExportKind::Named,
                    line,
                    reexport_source: None,
                    params: Vec::new(),
                    is_type_only: false,
                    local_name: None,
                });
            }
        }
        _ => {}
    }
}

fn oxc_extract_export_all(
    lines: &[usize],
    export: &oxc_ast::ast::ExportAllDeclaration,
    out: &mut Vec<ExportedSymbol>,
) {
    let line = oxc_line_at(lines, export.span.start as usize);
    let reexport_source = export.source.value.as_str().to_string();

    // `export * as ns from '...'` (namespace re-export) creates a single named
    // export `ns`, not a wildcard. Only bare `export * from '...'` (no `as ns`)
    // re-exports every name and becomes a `StarReExport`.
    if let Some(exported) = export.exported.as_ref() {
        if let Some(name) = exported.identifier_name() {
            out.push(ExportedSymbol {
                name: name.as_str().to_string(),
                kind: ExportKind::ReExport,
                line,
                reexport_source: Some(reexport_source),
                params: Vec::new(),
                is_type_only: false,
                local_name: None,
            });
        }
        // A string-literal namespace name (`export * as "ns"`) has no
        // `identifier_name`; it is unreachable as an identifier import, so it
        // contributes no enumerable named export.
        return;
    }
    out.push(ExportedSymbol {
        name: "*".into(),
        kind: ExportKind::StarReExport,
        line,
        reexport_source: Some(reexport_source),
        params: Vec::new(),
        is_type_only: false,
        local_name: None,
    });
}

/// Function-declaration params whose pattern is a plain identifier — matches
/// tree-sitter `extract_params` (destructured / rest params are skipped).
fn oxc_extract_params(func: &oxc_ast::ast::Function) -> Vec<String> {
    use oxc_ast::ast::BindingPattern;
    let mut result = Vec::new();
    for item in &func.params.items {
        if let BindingPattern::BindingIdentifier(id) = &item.pattern {
            result.push(id.name.as_str().to_string());
        }
    }
    result
}

/// oxc equivalent of tree-sitter `collect_pattern_names` over a binding pattern.
fn oxc_collect_pattern_names(pattern: &oxc_ast::ast::BindingPattern, out: &mut Vec<String>) {
    use oxc_ast::ast::BindingPattern;
    match pattern {
        BindingPattern::BindingIdentifier(id) => out.push(id.name.as_str().to_string()),
        BindingPattern::ObjectPattern(obj) => {
            // `value` is the actual binding for both `{ a }` (shorthand) and
            // `{ a: local }` (pair). Rest comes last in source order.
            for prop in &obj.properties {
                oxc_collect_pattern_names(&prop.value, out);
            }
            if let Some(rest) = &obj.rest {
                oxc_collect_pattern_names(&rest.argument, out);
            }
        }
        BindingPattern::ArrayPattern(arr) => {
            for elem in arr.elements.iter().flatten() {
                oxc_collect_pattern_names(elem, out);
            }
            if let Some(rest) = &arr.rest {
                oxc_collect_pattern_names(&rest.argument, out);
            }
        }
        BindingPattern::AssignmentPattern(assign) => {
            // `{ a = 1 }` / `[a = 1]` — the left side is the binding.
            oxc_collect_pattern_names(&assign.left, out);
        }
    }
}

fn oxc_extract_call_new(
    lines: &[usize],
    new_expr: &oxc_ast::ast::NewExpression,
    out: &mut Vec<LocalCall>,
) {
    oxc_push_call(
        lines,
        &new_expr.callee,
        &new_expr.arguments,
        new_expr.span.start as usize,
        new_expr.span.end as usize,
        CallKind::New,
        out,
    );
}

fn oxc_extract_call_call(
    lines: &[usize],
    call: &oxc_ast::ast::CallExpression,
    out: &mut Vec<LocalCall>,
) {
    oxc_push_call(
        lines,
        &call.callee,
        &call.arguments,
        call.span.start as usize,
        call.span.end as usize,
        CallKind::Call,
        out,
    );
}

/// Shared body for `new X(...)` / `X(...)`. Only fires when the callee is a
/// bare identifier; argument names are `Some` for plain identifiers, `None`
/// otherwise (spreads, member access, literals, …).
fn oxc_push_call(
    lines: &[usize],
    callee: &oxc_ast::ast::Expression,
    arguments: &[oxc_ast::ast::Argument],
    start: usize,
    end: usize,
    kind: CallKind,
    out: &mut Vec<LocalCall>,
) {
    use oxc_ast::ast::{Argument, Expression};

    let Expression::Identifier(id) = callee else {
        return;
    };
    let args = arguments
        .iter()
        .map(|arg| match arg {
            // tree-sitter parses `undefined` as its own node kind (not an
            // identifier), so it yields `None`; match that here.
            Argument::Identifier(id) if id.name.as_str() != "undefined" => {
                Some(id.name.as_str().to_string())
            }
            _ => None,
        })
        .collect();

    out.push(LocalCall {
        local_name: id.name.as_str().to_string(),
        line: oxc_line_at(lines, start),
        column: oxc_column_at(lines, start),
        byte_offset: start,
        byte_len: end - start,
        kind,
        args,
    });
}

fn oxc_extract_dynamic_import(
    lines: &[usize],
    import_expr: &oxc_ast::ast::ImportExpression,
    out: &mut Vec<ImportedSymbol>,
    dyn_dirs: &mut Vec<String>,
) {
    use oxc_ast::ast::Expression;
    // `import(\`./locales/${lang}\`)` — the path is computed at runtime, so it
    // names a directory rather than one file. Record the static directory prefix.
    if let Expression::TemplateLiteral(tpl) = &import_expr.source {
        if !tpl.expressions.is_empty()
            && let Some(first) = tpl.quasis.first()
            && let Some(dir) = template_static_dir(first.value.raw.as_str())
        {
            dyn_dirs.push(dir);
        }
        return;
    }
    let Expression::StringLiteral(lit) = &import_expr.source else {
        return;
    };
    let specifier = lit.value.as_str();
    if specifier.is_empty() {
        return;
    }
    out.push(ImportedSymbol {
        local_name: String::new(),
        imported_name: "*".into(),
        kind: ImportKind::Namespace,
        specifier: specifier.to_string(),
        source_path: None,
        line: oxc_line_at(lines, import_expr.span.start as usize),
        is_type_only: false,
    });
}

fn oxc_extract_require(
    lines: &[usize],
    call: &oxc_ast::ast::CallExpression,
    out: &mut Vec<ImportedSymbol>,
) {
    use oxc_ast::ast::{Argument, Expression};
    let Expression::Identifier(callee) = &call.callee else {
        return;
    };
    if callee.name.as_str() != "require" {
        return;
    }
    let Some(first_arg) = call.arguments.first() else {
        return;
    };
    let Argument::StringLiteral(lit) = first_arg else {
        return;
    };
    let specifier = lit.value.as_str();
    if specifier.is_empty() {
        return;
    }
    out.push(ImportedSymbol {
        local_name: String::new(),
        imported_name: "*".into(),
        kind: ImportKind::Namespace,
        specifier: specifier.to_string(),
        source_path: None,
        line: oxc_line_at(lines, call.span.start as usize),
        is_type_only: false,
    });
}

/// Try to resolve a relative specifier (`./foo`, `../bar/baz`) into an
/// absolute path that appears in the input set. Bare specifiers and
/// non-resolving ones return `None`.
fn resolve_relative(
    importer: &Path,
    specifier: &str,
    known: &std::collections::HashSet<PathBuf>,
) -> Option<PathBuf> {
    if !specifier.starts_with('.') {
        return None;
    }
    // Drop a build-tool query/hash suffix (`./checks.js?worker`, `./mod.ts#frag`):
    // the bundler consumes the directive at build time; only the bare path exists
    // on disk.
    let specifier = crate::rules::path_utils::strip_specifier_query(specifier);
    let base_dir = importer.parent()?;
    let target = base_dir.join(specifier);
    probe_path(&target, known).or_else(|| probe_decl_sibling(&target))
}

/// Resolve a template-literal dynamic-import directory prefix (`../locales`,
/// `./pages`) against `importer`'s directory into a canonical directory.
/// Falls back to lexical normalization when the directory does not exist on
/// disk (e.g. unit tests over synthetic paths). Returns `None` when the
/// importer has no parent.
fn resolve_dynamic_dir(importer: &Path, rel: &str) -> Option<PathBuf> {
    let base_dir = importer.parent()?;
    let joined = base_dir.join(rel);
    Some(std::fs::canonicalize(&joined).unwrap_or_else(|_| lexical_normalize(&joined)))
}

/// Probe an absolute path (without or with extension) against the known set,
/// trying bare, each TS/JS extension, and `index.*` variants.
/// Resolve `.`/`..` components purely lexically — no filesystem access, no
/// symlink resolution. Used as a fast pre-check against the canonical `known`
/// set before falling back to `std::fs::canonicalize`.
fn lexical_normalize(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut pb = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                pb.pop();
            }
            other => pb.push(other.as_os_str()),
        }
    }
    pb
}

/// TypeScript source extensions to try when an import specifier carries a
/// JS-family extension. TypeScript ESM (`"module": "NodeNext"` / `"ESNext"`)
/// requires writing the emitted `.js` extension in specifiers even when the
/// on-disk source is `.ts`, so `./checks.js` must resolve to `checks.ts`.
fn ts_counterpart_exts(ext: &str) -> &'static [&'static str] {
    match ext {
        "js" | "jsx" => &["ts", "tsx"],
        "mjs" => &["mts"],
        "cjs" => &["cts"],
        _ => &[],
    }
}

/// Read the `name` field of a `package.json`, or `None` when the file is
/// absent, unparseable, or declares no `name`.
fn read_package_name(manifest: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(manifest).ok()?;
    crate::project::PackageJson::parse(&raw)?.name
}

fn probe_path(raw: &Path, known: &std::collections::HashSet<PathBuf>) -> Option<PathBuf> {
    const EXTS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "mjs", "cts", "cjs", "vue"];
    // Declaration-only directory entries. TypeScript resolves `import './dir'`
    // to `./dir/index.d.ts` when the directory holds only a declaration file.
    // These never appear in `known` (`.d.*` files are excluded from the scan
    // set), so they need an existence-only check rather than a `known` lookup.
    const INDEX_DECL_FILES: &[&str] = &["index.d.ts", "index.d.mts", "index.d.cts"];

    // Explicit .d.* imports: existence-only check, no `known` membership required.
    // .d.ts files are intentionally excluded from the scan set but are valid import targets.
    if raw
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| {
            n.ends_with(".d.ts")
                || n.ends_with(".d.mts")
                || n.ends_with(".d.cts")
                || n.ends_with(".d.tsx")
        })
    {
        return std::fs::canonicalize(raw).ok();
    }

    let known_ext = raw
        .extension()
        .and_then(|e| e.to_str())
        .filter(|ext| EXTS.contains(ext));

    // Fast pass: try every candidate the syscall pass below would, but resolve
    // it lexically and look it up in the in-memory `known` set. `raw` is built
    // from a canonical importer dir, so a normalized candidate present in
    // `known` is exactly what `canonicalize` would return — and candidates are
    // tried in the same priority order, so the result is identical. Anything
    // reachable only through a symlink misses here and falls to the syscall
    // pass, preserving behavior.
    if let Some(ext) = known_ext {
        let n = lexical_normalize(raw);
        if known.contains(&n) {
            return Some(n);
        }
        for ts_ext in ts_counterpart_exts(ext) {
            let n = lexical_normalize(&raw.with_extension(ts_ext));
            if known.contains(&n) {
                return Some(n);
            }
        }
    } else {
        if raw.extension().is_some() {
            if let Some(raw_str) = raw.to_str() {
                for ext in EXTS {
                    let n = lexical_normalize(&PathBuf::from(format!("{raw_str}.{ext}")));
                    if known.contains(&n) {
                        return Some(n);
                    }
                }
            }
        }
        for ext in EXTS {
            let n = lexical_normalize(&raw.with_extension(ext));
            if known.contains(&n) {
                return Some(n);
            }
        }
        for ext in EXTS {
            let n = lexical_normalize(&raw.join(format!("index.{ext}")));
            if known.contains(&n) {
                return Some(n);
            }
        }
    }

    if let Some(ext) = known_ext {
        if let Ok(c) = std::fs::canonicalize(raw)
            && known.contains(&c)
        {
            return Some(c);
        }
        for ts_ext in ts_counterpart_exts(ext) {
            if let Ok(c) = std::fs::canonicalize(raw.with_extension(ts_ext))
                && known.contains(&c)
            {
                return Some(c);
            }
        }
        return None;
    }

    // When the raw path already ends with an unknown "extension" component
    // (e.g., `$cabinetId` in TanStack Router filenames like
    // `cabinets_.$cabinetId`), `with_extension` would replace that segment
    // and produce the wrong candidate (`cabinets_.tsx`).  Try appending the
    // TS/JS extension to the full path first so that `./cabinets_.$cabinetId`
    // resolves to `cabinets_.$cabinetId.tsx`.
    let has_unknown_ext = raw.extension().is_some();
    if has_unknown_ext {
        for ext in EXTS {
            if let Some(raw_str) = raw.to_str() {
                let candidate = PathBuf::from(format!("{raw_str}.{ext}"));
                if let Ok(c) = std::fs::canonicalize(&candidate)
                    && known.contains(&c)
                {
                    return Some(c);
                }
            }
        }
    }

    for ext in EXTS {
        let candidate = raw.with_extension(ext);
        if let Ok(c) = std::fs::canonicalize(&candidate)
            && known.contains(&c)
        {
            return Some(c);
        }
    }
    for ext in EXTS {
        let candidate = raw.join(format!("index.{ext}"));
        if let Ok(c) = std::fs::canonicalize(&candidate)
            && known.contains(&c)
        {
            return Some(c);
        }
    }
    // Declaration-only directory: `./dir` → `./dir/index.d.ts`. Existence-only,
    // since `.d.*` files are not in `known`.
    for decl in INDEX_DECL_FILES {
        if let Ok(c) = std::fs::canonicalize(raw.join(decl)) {
            return Some(c);
        }
    }
    None
}

/// Resolve an extensionless import target to its declaration-file sibling
/// (`./types` → `./types.d.ts`, `.d.mts`, or `.d.cts`). TypeScript resolves a
/// bare specifier to a declaration-only sibling when no source file exists.
/// Existence-only, since `.d.*` files are excluded from the indexed `known` set.
/// Applies to relative and tsconfig-alias specifiers, which name a concrete
/// target; package resolution (`main`/`types`) is handled by the resolver, so
/// this is not used there.
fn probe_decl_sibling(raw: &Path) -> Option<PathBuf> {
    const DECL_EXTS: &[&str] = &[".d.ts", ".d.mts", ".d.cts"];
    let raw_str = raw.to_str()?;
    DECL_EXTS
        .iter()
        .find_map(|decl| std::fs::canonicalize(PathBuf::from(format!("{raw_str}{decl}"))).ok())
}

/// Try to resolve a specifier into an absolute path that appears in the input
/// set. Relative specifiers (`./foo`) take a fast in-memory path through
/// [`resolve_relative`]; bare and aliased specifiers are delegated to
/// [`OxcPathResolver`], which handles tsconfig `paths`, `baseUrl`, package
/// `exports`, and `node_modules` walking.
fn resolve_specifier(
    importer: &Path,
    specifier: &str,
    known: &std::collections::HashSet<PathBuf>,
    resolver: &OxcPathResolver,
) -> Option<PathBuf> {
    if specifier.starts_with('.') {
        return resolve_relative(importer, specifier, known);
    }
    resolver.resolve(importer, specifier, known)
}

/// Resolver for TS/JS module specifiers. Discovers every `tsconfig.json`
/// reachable from the indexed files, reads its `paths` mappings, and
/// resolves path aliases (`@/*`, `~/*`, …) in-process. Non-alias bare
/// specifiers fall through to `oxc_resolver` for `node_modules` lookup.
#[derive(Debug, Default)]
struct OxcPathResolver {
    /// (tsconfig_dir, path_aliases, oxc_resolver) sorted longest-path-first.
    resolvers: Vec<TsconfigResolver>,
    fallback: Option<Resolver>,
    /// Workspace member package `name` → directory of its `package.json`,
    /// sorted longest-name-first so a scoped name (`@scope/pkg`) is tried before
    /// a shorter prefix. Lets a bare specifier that names a workspace sibling
    /// (`import x from "motion-utils"`) resolve to that package's on-disk source
    /// entry — the published `main`/`exports` point at compiled `dist/` output
    /// that is not in the indexed source set, so oxc resolution alone would
    /// leave the sibling's source files unreferenced.
    workspace_packages: Vec<(String, PathBuf)>,
}

#[derive(Debug)]
struct TsconfigResolver {
    dir: PathBuf,
    aliases: Vec<(String, Vec<PathBuf>)>,
    oxc: Resolver,
}

impl OxcPathResolver {
    fn discover(known_paths: &std::collections::HashSet<PathBuf>) -> Self {
        let mut seen_dirs: HashSet<PathBuf> = HashSet::new();
        let mut tsconfig_dirs: HashMap<PathBuf, PathBuf> = HashMap::new();
        // Workspace member name → its manifest directory. Built from every
        // named `package.json` reachable above an indexed file.
        let mut workspace_packages: HashMap<String, PathBuf> = HashMap::new();

        for path in known_paths {
            let Some(mut dir) = path.parent() else {
                continue;
            };
            loop {
                if seen_dirs.contains(dir) {
                    break;
                }
                seen_dirs.insert(dir.to_path_buf());
                let candidate = dir.join("tsconfig.json");
                if candidate.exists() {
                    tsconfig_dirs.entry(dir.to_path_buf()).or_insert(candidate);
                }
                if let Some(name) = read_package_name(&dir.join("package.json")) {
                    workspace_packages.entry(name).or_insert_with(|| dir.to_path_buf());
                }
                let Some(parent) = dir.parent() else { break };
                dir = parent;
            }
        }

        // Longest name first so `@scope/pkg` wins over a `@scope` prefix when a
        // specifier could match either.
        let mut workspace_packages: Vec<(String, PathBuf)> =
            workspace_packages.into_iter().collect();
        workspace_packages.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        let mut resolvers: Vec<TsconfigResolver> = tsconfig_dirs
            .into_iter()
            .map(|(dir, tsconfig_path)| {
                let aliases = Self::read_path_aliases(&dir, &tsconfig_path);
                let oxc = Self::make_oxc(Some(tsconfig_path));
                TsconfigResolver { dir, aliases, oxc }
            })
            .collect();

        resolvers.sort_by(|a, b| b.dir.as_os_str().len().cmp(&a.dir.as_os_str().len()));

        let fallback = Some(Self::make_oxc(None));

        Self {
            resolvers,
            fallback,
            workspace_packages,
        }
    }

    fn read_path_aliases(tsconfig_dir: &Path, tsconfig_path: &Path) -> Vec<(String, Vec<PathBuf>)> {
        let raw = match std::fs::read_to_string(tsconfig_path) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        // parse (not load_file) intentionally: extends-inherited paths
        // would be resolved relative to the child dir, not the parent's.
        // Each tsconfig in the tree gets its own resolver entry, so
        // parent-defined aliases are handled by the parent's entry.
        let tsconfig = match crate::project::Tsconfig::parse(&raw) {
            Some(t) => t,
            None => return Vec::new(),
        };
        let base = tsconfig
            .base_url
            .as_ref()
            .map(|b| tsconfig_dir.join(b))
            .unwrap_or_else(|| tsconfig_dir.to_path_buf());

        tsconfig
            .paths
            .into_iter()
            .map(|(pattern, targets)| {
                let resolved_targets: Vec<PathBuf> =
                    targets.into_iter().map(|t| base.join(t)).collect();
                (pattern, resolved_targets)
            })
            .collect()
    }

    fn make_oxc(tsconfig_path: Option<PathBuf>) -> Resolver {
        let options = ResolveOptions {
            extensions: vec![
                ".ts".into(),
                ".tsx".into(),
                ".js".into(),
                ".jsx".into(),
                ".mts".into(),
                ".mjs".into(),
                ".cts".into(),
                ".cjs".into(),
                ".vue".into(),
            ],
            condition_names: vec![
                "import".into(),
                "require".into(),
                "node".into(),
                "default".into(),
            ],
            tsconfig: tsconfig_path.map(|p| {
                TsconfigDiscovery::Manual(TsconfigOptions {
                    config_file: p,
                    references: TsconfigReferences::Auto,
                })
            }),
            ..Default::default()
        };
        Resolver::new(options)
    }

    fn resolve(
        &self,
        importer: &Path,
        specifier: &str,
        known: &std::collections::HashSet<PathBuf>,
    ) -> Option<PathBuf> {
        let entry = self.resolvers.iter().find(|e| importer.starts_with(&e.dir));

        // Try tsconfig path aliases first.
        if let Some(e) = entry
            && let Some(resolved) = Self::resolve_alias(specifier, &e.aliases, known) {
                return Some(resolved);
            }

        // Cross-package workspace import by name: a bare specifier whose head is
        // a sibling package's `name` resolves to that package's on-disk source,
        // not its published `dist/` entry (which oxc would find but isn't in the
        // indexed source set).
        if let Some(resolved) = self.resolve_workspace_package(specifier, known) {
            return Some(resolved);
        }

        // Fall through to oxc_resolver for node_modules / other resolution.
        let oxc = entry.map(|e| &e.oxc).or(self.fallback.as_ref())?;
        let importer_dir = importer.parent()?;
        let resolved = oxc.resolve(importer_dir, specifier).ok()?;
        let canonical = std::fs::canonicalize(resolved.path()).ok()?;
        known.contains(&canonical).then_some(canonical)
    }

    /// Resolve a bare specifier that names a workspace sibling package to its
    /// on-disk source entry. `motion-utils` → `<pkg-dir>/src/index.ts`;
    /// `@scope/pkg/foo/bar` → the source file under `<pkg-dir>` for subpath
    /// `foo/bar`. Returns `None` when the specifier matches no member or no
    /// candidate source file exists in the indexed set.
    fn resolve_workspace_package(
        &self,
        specifier: &str,
        known: &std::collections::HashSet<PathBuf>,
    ) -> Option<PathBuf> {
        for (name, dir) in &self.workspace_packages {
            let subpath = if specifier == name {
                ""
            } else if let Some(rest) = specifier
                .strip_prefix(name)
                .and_then(|r| r.strip_prefix('/'))
            {
                rest
            } else {
                continue;
            };

            // Bare package name → the package's source index. A subpath →
            // that path under the package dir (with or without a `src/` root).
            let candidates: Vec<PathBuf> = if subpath.is_empty() {
                vec![dir.join("src").join("index"), dir.join("index")]
            } else {
                vec![dir.join("src").join(subpath), dir.join(subpath)]
            };
            for candidate in candidates {
                if let Some(resolved) = probe_path(&candidate, known) {
                    return Some(resolved);
                }
            }
        }
        None
    }

    fn resolve_alias(
        specifier: &str,
        aliases: &[(String, Vec<PathBuf>)],
        known: &std::collections::HashSet<PathBuf>,
    ) -> Option<PathBuf> {
        for (pattern, targets) in aliases {
            let matched_suffix = if let Some(prefix) = pattern.strip_suffix('*') {
                specifier.strip_prefix(prefix)
            } else if pattern == specifier {
                Some("")
            } else {
                None
            };
            let Some(suffix) = matched_suffix else {
                continue;
            };
            for target in targets {
                let target_str = target.to_string_lossy();
                let expanded = if let Some(base) = target_str.strip_suffix('*') {
                    PathBuf::from(format!("{base}{suffix}"))
                } else {
                    target.clone()
                };
                if let Some(resolved) =
                    probe_path(&expanded, known).or_else(|| probe_decl_sibling(&expanded))
                {
                    return Some(resolved);
                }
            }
        }
        None
    }
}

// -------------------------- Rust extraction --------------------------

/// Walk a parsed Rust source file and collect `pub` item exports + `use`
/// declaration imports. The specifier on Rust imports is the full path as
/// written (e.g. `crate::a::b::Sym`); resolution to a file path happens in a
/// separate pass using `RustModuleGraph`.
fn extract_rust(
    tree: &tree_sitter::Tree,
    source: &[u8],
) -> (Vec<ExportedSymbol>, Vec<ImportedSymbol>) {
    let mut exports = Vec::new();
    let mut imports = Vec::new();
    walk_tree(tree, |node| match node.kind() {
        "function_item" | "struct_item" | "enum_item" | "trait_item" | "type_item"
        | "const_item" | "static_item" | "mod_item" => {
            extract_rust_item(node, source, &mut exports)
        }
        "use_declaration" => extract_rust_use(node, source, &mut exports, &mut imports),
        _ => {}
    });
    (exports, imports)
}

/// Emit an export for a `pub`-qualified item. Non-`pub` items are ignored —
/// they aren't reachable across modules. `pub(crate)` / `pub(super)` are
/// treated like `pub` here: the index answers "can this be referenced from
/// another file?", not Rust's full visibility lattice.
fn extract_rust_item(node: Node, source: &[u8], out: &mut Vec<ExportedSymbol>) {
    if !rust_has_pub(node) {
        return;
    }
    let Some(name) = rust_item_name(node, source) else {
        return;
    };
    let kind = if node.kind() == "mod_item" {
        ExportKind::Module
    } else {
        ExportKind::Named
    };
    out.push(ExportedSymbol {
        name,
        kind,
        line: node.start_position().row + 1,
        reexport_source: None,
        params: Vec::new(),
        is_type_only: false,
        local_name: None,
    });
}

fn rust_has_pub(node: Node) -> bool {
    node.children(&mut node.walk())
        .any(|c| c.kind() == "visibility_modifier")
}

/// Name child is `identifier` for functions/consts/statics/mods,
/// `type_identifier` for types/structs/enums/traits.
fn rust_item_name(node: Node, source: &[u8]) -> Option<String> {
    let name_node = node
        .named_children(&mut node.walk())
        .find(|c| matches!(c.kind(), "identifier" | "type_identifier"))?;
    Some(text_of(name_node, source))
}

/// Handle `use` declarations. `pub use foo::Bar` emits a ReExport export (the
/// brought-in name is re-exposed by this module); all `use …` emit one or
/// more imports, one per leaf symbol.
fn extract_rust_use(
    node: Node,
    source: &[u8],
    exports: &mut Vec<ExportedSymbol>,
    imports: &mut Vec<ImportedSymbol>,
) {
    let is_pub = rust_has_pub(node);
    let line = node.start_position().row + 1;

    // The body is the first named child that isn't the visibility modifier:
    // a scoped_identifier, scoped_use_list, use_wildcard, or use_as_clause.
    let body = node
        .named_children(&mut node.walk())
        .find(|c| c.kind() != "visibility_modifier");
    let Some(body) = body else { return };

    let mut leaves: Vec<RustUseLeaf> = Vec::new();
    collect_rust_use_leaves(body, source, &[], &mut leaves);

    for leaf in leaves {
        // Rebuild the specifier as `segment::segment::name`.
        let specifier = if leaf.prefix.is_empty() {
            leaf.imported.clone()
        } else {
            format!("{}::{}", leaf.prefix.join("::"), leaf.imported)
        };

        let kind = if leaf.imported == "*" {
            ImportKind::Namespace
        } else {
            ImportKind::Named
        };

        imports.push(ImportedSymbol {
            local_name: leaf.local.clone(),
            imported_name: leaf.imported.clone(),
            kind,
            specifier: specifier.clone(),
            source_path: None,
            line,
            is_type_only: false,
        });

        if is_pub && leaf.imported != "*" {
            exports.push(ExportedSymbol {
                name: leaf.local.clone(),
                kind: ExportKind::ReExport,
                line,
                reexport_source: Some(specifier),
                params: Vec::new(),
                is_type_only: false,
                local_name: None,
            });
        }
    }
}

#[derive(Debug, Clone)]
struct RustUseLeaf {
    /// Path segments up to (but not including) the leaf.
    prefix: Vec<String>,
    /// Original name at the source (the leaf identifier, or `*`).
    imported: String,
    /// Local binding name — equal to `imported` unless renamed via `as`.
    local: String,
}

fn collect_rust_use_leaves(
    node: Node,
    source: &[u8],
    prefix: &[String],
    out: &mut Vec<RustUseLeaf>,
) {
    match node.kind() {
        "scoped_identifier" => {
            let segments = rust_scoped_segments(node, source);
            if segments.is_empty() {
                return;
            }
            let mut full = prefix.to_vec();
            full.extend(segments.iter().take(segments.len() - 1).cloned());
            let leaf = segments.last().cloned().unwrap_or_default();
            out.push(RustUseLeaf {
                prefix: full,
                imported: leaf.clone(),
                local: leaf,
            });
        }
        "identifier" | "type_identifier" => {
            let name = text_of(node, source);
            out.push(RustUseLeaf {
                prefix: prefix.to_vec(),
                imported: name.clone(),
                local: name,
            });
        }
        "scoped_use_list" => {
            // `a::b::{ X, Y }` — collect `a::b` as prefix, recurse into use_list.
            let mut inner_prefix = prefix.to_vec();
            if let Some(p) = node.named_children(&mut node.walk()).find(|c| {
                matches!(
                    c.kind(),
                    "scoped_identifier" | "identifier" | "self" | "crate" | "super"
                )
            }) {
                if p.kind() == "scoped_identifier" {
                    inner_prefix.extend(rust_scoped_segments(p, source));
                } else {
                    inner_prefix.push(text_of(p, source));
                }
            }
            if let Some(list) = node
                .named_children(&mut node.walk())
                .find(|c| c.kind() == "use_list")
            {
                for child in list.named_children(&mut list.walk()) {
                    collect_rust_use_leaves(child, source, &inner_prefix, out);
                }
            }
        }
        "use_list" => {
            for child in node.named_children(&mut node.walk()) {
                collect_rust_use_leaves(child, source, prefix, out);
            }
        }
        "use_wildcard" => {
            // `a::b::*` — subtree is a path followed by a `*` token.
            let mut inner_prefix = prefix.to_vec();
            if let Some(path) = node.named_children(&mut node.walk()).find(|c| {
                matches!(
                    c.kind(),
                    "scoped_identifier" | "identifier" | "self" | "crate" | "super"
                )
            }) {
                if path.kind() == "scoped_identifier" {
                    inner_prefix.extend(rust_scoped_segments(path, source));
                } else {
                    inner_prefix.push(text_of(path, source));
                }
            }
            out.push(RustUseLeaf {
                prefix: inner_prefix,
                imported: "*".into(),
                local: "*".into(),
            });
        }
        "use_as_clause" => {
            // `path as Alias` — first named child is the path, a later named
            // identifier is the alias.
            let path = node.named_children(&mut node.walk()).next();
            let alias = node
                .named_children(&mut node.walk())
                .skip(1)
                .find(|c| matches!(c.kind(), "identifier" | "type_identifier"));
            if let (Some(path), Some(alias)) = (path, alias) {
                let alias_name = text_of(alias, source);
                let mut tmp = Vec::new();
                collect_rust_use_leaves(path, source, prefix, &mut tmp);
                if let Some(mut leaf) = tmp.pop() {
                    leaf.local = alias_name;
                    out.push(leaf);
                }
            }
        }
        _ => {}
    }
}

/// Flatten a `scoped_identifier` AST node into its segment strings in
/// left-to-right order. `self` / `crate` / `super` are kept verbatim as
/// leading segments.
fn rust_scoped_segments(node: Node, source: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    rust_scoped_segments_into(node, source, &mut out);
    out
}

fn rust_scoped_segments_into(node: Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "scoped_identifier" => {
            for child in node.named_children(&mut node.walk()) {
                rust_scoped_segments_into(child, source, out);
            }
        }
        "identifier" | "type_identifier" | "self" | "crate" | "super" => {
            out.push(text_of(node, source));
        }
        _ => {}
    }
}

// -------------------------- Rust module graph --------------------------

/// Minimal module graph for resolving Rust `use` paths to files.
///
/// We don't implement a full name resolver — just enough to answer "given the
/// importing file and the path segments after `crate::` / `super::` / `self::`,
/// which file in the input set (if any) defines the last segment?".
#[derive(Debug, Default)]
struct RustModuleGraph {
    /// For each indexed `.rs` file, the crate root it belongs to (the nearest
    /// `lib.rs` / `main.rs` ancestor).
    crate_root: HashMap<PathBuf, PathBuf>,
    /// `(file, child_mod_name)` → resolved child file. Built from `pub mod foo;`
    /// declarations found during extraction.
    children: HashMap<(PathBuf, String), PathBuf>,
}

impl RustModuleGraph {
    fn build(
        per_file: &[(PathBuf, FileExtract)],
        known_paths: &std::collections::HashSet<PathBuf>,
    ) -> Self {
        let rust_files: Vec<&PathBuf> = per_file
            .iter()
            .filter(|(p, _)| matches!(p.extension().and_then(|e| e.to_str()), Some("rs")))
            .map(|(p, _)| p)
            .collect();

        // `declared_mods[parent_file]` lists the mod names declared by that
        // file. We surface only `pub mod` here (private `mod foo;` isn't
        // tracked by the exports list); that's enough for cross-crate-internal
        // resolution since `use crate::foo::…` requires `foo` to be visible.
        let mut declared_mods: HashMap<PathBuf, Vec<String>> = HashMap::new();
        for (path, extract) in per_file {
            if !matches!(path.extension().and_then(|e| e.to_str()), Some("rs")) {
                continue;
            }
            let mods: Vec<String> = extract
                .exports
                .iter()
                .filter(|e| e.kind == ExportKind::Module)
                .map(|e| e.name.clone())
                .collect();
            if !mods.is_empty() {
                declared_mods.insert(path.clone(), mods);
            }
        }

        // Identify crate roots: lib.rs / main.rs files.
        let crate_roots: Vec<&PathBuf> = rust_files
            .iter()
            .copied()
            .filter(|p| {
                matches!(
                    p.file_name().and_then(|n| n.to_str()),
                    Some("lib.rs") | Some("main.rs")
                )
            })
            .collect();

        // For each rust file, find its owning crate root: the deepest crate
        // root whose directory is an ancestor of the file.
        let mut crate_root = HashMap::new();
        for f in &rust_files {
            let mut best: Option<&PathBuf> = None;
            for root in &crate_roots {
                let Some(root_dir) = root.parent() else {
                    continue;
                };
                if f.starts_with(root_dir)
                    && best.is_none_or(|b: &PathBuf| {
                        b.parent().is_some_and(|bd| root_dir.starts_with(bd))
                    })
                {
                    best = Some(root);
                }
            }
            if let Some(root) = best {
                crate_root.insert((*f).clone(), root.clone());
            }
        }

        // Build children: for each file declaring `mod foo;`, probe
        // `foo.rs` / `foo/mod.rs` in its child search directory.
        let mut children = HashMap::new();
        for (parent_file, mods) in &declared_mods {
            for dir in rust_child_search_dirs(parent_file) {
                for m in mods {
                    if children.contains_key(&(parent_file.clone(), m.clone())) {
                        continue;
                    }
                    let flat = dir.join(format!("{m}.rs"));
                    let modrs = dir.join(m).join("mod.rs");
                    for candidate in [&flat, &modrs] {
                        if let Ok(c) = std::fs::canonicalize(candidate)
                            && known_paths.contains(&c)
                        {
                            children.insert((parent_file.clone(), m.clone()), c);
                            break;
                        }
                    }
                }
            }
        }

        Self {
            crate_root,
            children,
        }
    }

    /// Resolve a Rust `use` specifier (e.g. `crate::a::b::Sym`, `super::X`,
    /// `self::y::Z`, `std::io::Read`) to the file that defines the last
    /// segment. Returns `None` for external crates or any segment that can't
    /// be found in the module graph.
    fn resolve(&self, importer: &Path, specifier: &str) -> Option<PathBuf> {
        let mut segs: Vec<&str> = specifier.split("::").collect();
        if segs.len() < 2 {
            return None;
        }

        let importer_buf = importer.to_path_buf();
        let mut current = match segs[0] {
            "crate" => {
                segs.remove(0);
                self.crate_root.get(&importer_buf)?.clone()
            }
            "self" => {
                segs.remove(0);
                importer_buf.clone()
            }
            "super" => {
                let mut cursor = self.parent_module(&importer_buf)?;
                segs.remove(0);
                while segs.first() == Some(&"super") {
                    cursor = self.parent_module(&cursor)?;
                    segs.remove(0);
                }
                cursor
            }
            _ => return None, // external crate (`serde`, `std`, …)
        };

        // All segments except the last are module steps; the last is the
        // exported symbol name in the resolved file.
        if segs.is_empty() {
            return None;
        }
        let _symbol = segs.pop()?;
        for seg in segs {
            if seg == "*" {
                return None;
            }
            match self.children.get(&(current.clone(), seg.to_string())) {
                Some(next) => current = next.clone(),
                None => return None,
            }
        }
        Some(current)
    }

    /// Return the module file one level above `file`. `mod.rs` / crate roots
    /// step up to their containing directory's module (the parent dir's
    /// `mod.rs`, sibling `<dir>.rs` Rust 2018 file, or crate root).
    fn parent_module(&self, file: &Path) -> Option<PathBuf> {
        // Crate roots have no parent module.
        if matches!(
            file.file_name().and_then(|n| n.to_str()),
            Some("lib.rs") | Some("main.rs")
        ) {
            return None;
        }
        let parent_dir = file.parent()?;

        // `mod.rs` → the declaring file lives one directory higher.
        // `x.rs` → the declaring file lives in the same directory (and the
        // module `x` itself lives inside `x/` if any; that `x/` is not the
        // declaring dir).
        let (declaring_dir, sibling_name) =
            if file.file_name().and_then(|n| n.to_str()) == Some("mod.rs") {
                (
                    parent_dir.parent()?,
                    parent_dir.file_name().and_then(|n| n.to_str()),
                )
            } else {
                (parent_dir, None)
            };

        // Rust 2018: `foo/mod.rs` can also be declared by a sibling `foo.rs`
        // at the grandparent level. Check that first, then the legacy `mod.rs`
        // and crate-root candidates.
        if let Some(name) = sibling_name {
            let candidate = declaring_dir.join(format!("{name}.rs"));
            if let Ok(c) = std::fs::canonicalize(&candidate) {
                return Some(c);
            }
        }
        for name in ["mod.rs", "lib.rs", "main.rs"] {
            let candidate = declaring_dir.join(name);
            if let Ok(c) = std::fs::canonicalize(&candidate) {
                return Some(c);
            }
        }

        // For a file like `a/b.rs`, the declaring module is either
        // `a/mod.rs` (already tried above) or the sibling `a.rs` next to `a/`.
        if let Some(dir_name) = parent_dir.file_name().and_then(|n| n.to_str())
            && let Some(grandparent) = parent_dir.parent()
        {
            let candidate = grandparent.join(format!("{dir_name}.rs"));
            if let Ok(c) = std::fs::canonicalize(&candidate) {
                return Some(c);
            }
        }
        None
    }
}

/// Directories to search for a `mod foo;` declared in `parent_file`:
/// - `lib.rs` / `main.rs` / `mod.rs` declare children in their own directory.
/// - A non-`mod.rs` file `x.rs` declares children in a sibling `x/` directory.
fn rust_child_search_dirs(parent_file: &Path) -> Vec<PathBuf> {
    let Some(parent_dir) = parent_file.parent() else {
        return Vec::new();
    };
    let name = parent_file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if matches!(name, "lib.rs" | "main.rs" | "mod.rs") {
        vec![parent_dir.to_path_buf()]
    } else {
        let stem = parent_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        vec![parent_dir.join(stem)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Build a tiny multi-file project under a tempdir and index it.
    fn build_index(files: &[(&str, &str)]) -> (TempDir, ImportIndex, Vec<PathBuf>) {
        let dir = TempDir::new().unwrap();
        let mut source_files = Vec::new();
        let mut paths = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            let lang = Language::from_path(&p).unwrap();
            source_files.push(SourceFile {
                path: p.clone(),
                language: lang,
            });
            paths.push(fs::canonicalize(&p).unwrap());
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let index = ImportIndex::build(&refs);
        (dir, index, paths)
    }

    // Regression for #1850: a bare specifier that names a workspace sibling
    // package resolves to that package's on-disk source entry, not its published
    // `dist/` entry — so the importer's `source_path` points at the sibling's
    // `src/index.ts` and the cross-package edge is recorded.
    #[test]
    fn cross_package_name_import_resolves_to_sibling_source_index() {
        let dir = TempDir::new().unwrap();
        // The depended-upon package: published entry points at dist/, source
        // lives in src/index.ts.
        fs::create_dir_all(dir.path().join("packages/lib/src")).unwrap();
        fs::write(
            dir.path().join("packages/lib/package.json"),
            r#"{"name":"@scope/lib","main":"./dist/index.js"}"#,
        )
        .unwrap();
        let lib_index = dir.path().join("packages/lib/src/index.ts");
        fs::write(&lib_index, "export const value = 1;\n").unwrap();

        // The consumer imports it by name.
        fs::create_dir_all(dir.path().join("packages/app/src")).unwrap();
        fs::write(
            dir.path().join("packages/app/package.json"),
            r#"{"name":"@scope/app"}"#,
        )
        .unwrap();
        let app_index = dir.path().join("packages/app/src/index.ts");
        fs::write(
            &app_index,
            "import { value } from '@scope/lib';\nexport const used = value;\n",
        )
        .unwrap();

        let lib_src = SourceFile {
            path: lib_index.clone(),
            language: Language::TypeScript,
        };
        let app_src = SourceFile {
            path: app_index.clone(),
            language: Language::TypeScript,
        };
        let refs = vec![&lib_src, &app_src];
        let index = ImportIndex::build(&refs);

        let canon_app = fs::canonicalize(&app_index).unwrap();
        let canon_lib = fs::canonicalize(&lib_index).unwrap();
        let imp = index
            .get_imports(&canon_app)
            .iter()
            .find(|i| i.specifier == "@scope/lib")
            .expect("import of @scope/lib must be indexed");
        assert_eq!(
            imp.source_path.as_ref(),
            Some(&canon_lib),
            "cross-package name import must resolve to the sibling's source index"
        );
    }

    // Regression for #1556: a `.md` / `.mdx` doc page imports a component via a
    // relative ESM specifier; the import edge is collected and resolved to the
    // component's source file exactly like a `.ts` import, so the component
    // counts as cross-file used.
    #[test]
    fn markdown_and_mdx_imports_are_collected_and_resolved() {
        let (_dir, index, paths) = build_index(&[
            (
                "components/DetailedExplanation.jsx",
                "export const DetailedExplanation = () => null;\n",
            ),
            (
                "docs/CodeStructure.md",
                "# Heading\n\nimport { DetailedExplanation } from '../components/DetailedExplanation'\n\n<DetailedExplanation />\n",
            ),
            (
                "docs/side-effects.mdx",
                "import { DetailedExplanation } from '../components/DetailedExplanation'\n\n<DetailedExplanation />\n",
            ),
        ]);

        let component = &paths[0];
        let md = &paths[1];
        let mdx = &paths[2];

        for (label, doc) in [("md", md), ("mdx", mdx)] {
            let imp = index
                .get_imports(doc)
                .iter()
                .find(|i| i.local_name == "DetailedExplanation")
                .unwrap_or_else(|| panic!("{label} import must be indexed"));
            assert_eq!(
                imp.source_path.as_ref(),
                Some(component),
                "{label} import must resolve to the component source"
            );
        }

        // The named import flows into the per-symbol usage map, so dead-export
        // sees the component as used.
        assert_eq!(
            index.get_usages(component, "DetailedExplanation").len(),
            2,
            "both doc pages register a usage of the component"
        );
    }

    // Prose containing the word "import" and `import` lines inside fenced code
    // blocks are not real ESM imports — they must not produce edges.
    #[test]
    fn markdown_prose_and_code_fences_are_not_treated_as_imports() {
        let (_dir, index, paths) = build_index(&[
            ("lib/widget.ts", "export const Widget = 1;\n"),
            (
                "docs/guide.md",
                "You can import the widget when needed.\n\n```ts\nimport { Widget } from '../lib/widget'\n```\n",
            ),
        ]);

        let doc = &paths[1];
        assert!(
            index.get_imports(doc).is_empty(),
            "prose and fenced-code imports must not create edges"
        );
    }

    /// Index a project reached through a symlinked directory, mirroring macOS
    /// where `/tmp` is a symlink to `/private/tmp`. Both the indexed file keys
    /// and the resolved import targets are canonicalized, so a relative import
    /// to a real on-disk file resolves regardless of the symlinked spelling —
    /// and the per-file `canonical(raw)` lookup the rules use lands on the same
    /// key. A genuinely missing target still stays unresolved.
    #[cfg(unix)]
    #[test]
    fn symlinked_project_resolves_relative_imports_issue_2029() {
        let real = TempDir::new().unwrap();
        fs::create_dir_all(real.path().join("sub/deep")).unwrap();
        fs::write(real.path().join("errors.js"), "export const e = 1;").unwrap();
        fs::write(
            real.path().join("sub/deep/a.ts"),
            "import { e } from '../../errors.js';\nimport { z } from '../../missing.js';",
        )
        .unwrap();

        let link_parent = TempDir::new().unwrap();
        let link = link_parent.path().join("via_link");
        std::os::unix::fs::symlink(real.path(), &link).unwrap();

        let importer = SourceFile {
            path: link.join("sub/deep/a.ts"),
            language: Language::Tsx,
        };
        let target = SourceFile {
            path: link.join("errors.js"),
            language: Language::JavaScript,
        };
        let refs = vec![&importer, &target];
        let index = ImportIndex::build(&refs);

        let canon_target = std::fs::canonicalize(&target.path).unwrap();

        // The rule canonicalizes the raw (symlinked) importer path first; that
        // lookup must hit the index and surface the imports.
        let imports = index.get_imports(&index.canonical(&importer.path));
        assert_eq!(imports.len(), 2);

        let resolved = imports.iter().find(|i| i.local_name == "e").unwrap();
        assert_eq!(
            resolved.source_path.as_ref(),
            Some(&canon_target),
            "existing import through a symlinked dir must resolve"
        );

        let missing = imports.iter().find(|i| i.local_name == "z").unwrap();
        assert!(
            missing.source_path.is_none(),
            "genuinely missing import must stay unresolved"
        );
    }

    #[test]
    fn indexes_named_exports() {
        let (_dir, index, paths) = build_index(&[(
            "util.ts",
            "export const foo = 1; export function bar() {} export type Baz = number;",
        )]);
        let exports = index.get_exports(&paths[0]);
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
        assert!(names.contains(&"Baz"));
    }

    #[test]
    fn indexes_ambient_declare_exports_issue_2030() {
        // Regression for rbaumier/comply#2030 — ambient `export declare …`
        // declarations are body-less but importable named exports. `import-named`
        // reads this index, so every ambient form must be recorded or the import
        // is wrongly flagged "not exported".
        let (_dir, index, paths) = build_index(&[(
            "shared.ts",
            "export declare function test(name: string): void;\n\
             export declare const cfg: number;\n\
             export declare class Helper {}\n\
             export declare enum Mode { A, B }\n\
             export declare namespace NS { const x: number; }",
        )]);
        let exports = index.get_exports(&paths[0]);
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"test"), "ambient function must be indexed");
        assert!(names.contains(&"cfg"), "ambient const must be indexed");
        assert!(names.contains(&"Helper"), "ambient class must be indexed");
        assert!(names.contains(&"Mode"), "ambient enum must be indexed");
        assert!(names.contains(&"NS"), "ambient namespace must be indexed");
    }

    #[test]
    fn ambient_function_import_links_usage_issue_2030() {
        // The end-to-end shape from the issue: importing `{ test }` from a module
        // that exports it via `export declare function` must link as a usage (so
        // `import-named` sees the export) — while a name the module does NOT
        // export stays unlinked (true positive preserved).
        let (_dir, index, paths) = build_index(&[
            (
                "shared.ts",
                "export declare function test(name: string): void;",
            ),
            (
                "app.ts",
                "import { test, missing } from './shared.js';\ntest('x');",
            ),
        ]);
        let shared = &paths[0];
        assert_eq!(
            index.get_usages(shared, "test").len(),
            1,
            "ambient-exported `test` import must link to the exporting file"
        );
        // The export set drives `import-named`: `test` is present (no FP),
        // `missing` is absent (true positive stays). The usage map links by
        // import statement and does not encode export presence, so the export
        // set is what the rule checks against.
        let names: Vec<&str> = index
            .get_exports(shared)
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(names.contains(&"test"));
        assert!(!names.contains(&"missing"));
    }

    #[test]
    fn overload_signature_yields_single_export() {
        // A body-less overload signature paired with an implementation is one
        // export, not two — the ambient-declaration handling must not regress
        // this dedup.
        let (_dir, index, paths) = build_index(&[(
            "m.ts",
            "export function f(a: string): void;\n\
             export function f(a: number): void;\n\
             export function f(a: any): void {}",
        )]);
        let count = index
            .get_exports(&paths[0])
            .iter()
            .filter(|e| e.name == "f")
            .count();
        assert_eq!(count, 1, "overloaded export must yield exactly one export");
    }

    #[test]
    fn indexes_destructured_object_export() {
        // Regression for rbaumier/comply#37 — names introduced by a
        // destructured `export const { ... } = obj` are real exports.
        let (_dir, index, paths) = build_index(&[(
            "client.ts",
            "declare const authClient: any; export const { signIn, signOut, resetPassword } = authClient;",
        )]);
        let exports = index.get_exports(&paths[0]);
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"signIn"));
        assert!(names.contains(&"signOut"));
        assert!(names.contains(&"resetPassword"));
    }

    #[test]
    fn indexes_destructured_array_export() {
        let (_dir, index, paths) = build_index(&[(
            "m.ts",
            "declare const arr: any; export const [first, second] = arr;",
        )]);
        let exports = index.get_exports(&paths[0]);
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"first"));
        assert!(names.contains(&"second"));
    }

    #[test]
    fn indexes_renamed_destructured_export() {
        let (_dir, index, paths) = build_index(&[(
            "m.ts",
            "declare const obj: any; export const { foo: bar } = obj;",
        )]);
        let exports = index.get_exports(&paths[0]);
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        // `foo: bar` exports the local name `bar`, not `foo`.
        assert!(names.contains(&"bar"));
        assert!(!names.contains(&"foo"));
    }

    #[test]
    fn indexes_default_export() {
        let (_dir, index, paths) = build_index(&[("m.ts", "export default function hello() {}")]);
        let exports = index.get_exports(&paths[0]);
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].name, "default");
        assert_eq!(exports[0].kind, ExportKind::Default);
    }

    #[test]
    fn indexes_named_import_and_links_usage() {
        let (_dir, index, paths) = build_index(&[
            ("util.ts", "export const foo = 1;"),
            ("app.ts", "import { foo } from './util';\nfoo + 1;"),
        ]);
        let util = &paths[0];
        let app = &paths[1];

        let imports = index.get_imports(app);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].local_name, "foo");
        assert_eq!(imports[0].source_path.as_ref(), Some(util));

        let usages = index.get_usages(util, "foo");
        assert_eq!(usages.len(), 1);
        assert_eq!(&usages[0].importer, app);
    }

    #[test]
    fn handles_renamed_imports() {
        let (_dir, index, paths) = build_index(&[
            ("m.ts", "export const original = 1;"),
            ("a.ts", "import { original as renamed } from './m';"),
        ]);
        let imports = index.get_imports(&paths[1]);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].imported_name, "original");
        assert_eq!(imports[0].local_name, "renamed");

        // Usage keyed by the exported name, not the local.
        assert_eq!(index.get_usages(&paths[0], "original").len(), 1);
        assert!(index.get_usages(&paths[0], "renamed").is_empty());
    }

    #[test]
    fn default_import_links_to_default_export() {
        let (_dir, index, paths) = build_index(&[
            ("m.ts", "export default class Thing {}"),
            ("a.ts", "import Thing from './m';"),
        ]);
        assert_eq!(index.get_usages(&paths[0], "default").len(), 1);
    }

    #[test]
    fn namespace_import_does_not_link_individual_usages() {
        let (_dir, index, paths) = build_index(&[
            ("m.ts", "export const a = 1; export const b = 2;"),
            ("u.ts", "import * as ns from './m';"),
        ]);
        // Namespace imports don't populate per-symbol usages — too lossy —
        // but they DO show up in get_imports + get_importers.
        assert!(index.get_usages(&paths[0], "a").is_empty());
        let importers = index.get_importers(&paths[0]);
        assert_eq!(importers.len(), 1);
        assert_eq!(importers[0], paths[1]);
    }

    #[test]
    fn side_effect_import_is_indexed() {
        let (_dir, index, paths) =
            build_index(&[("m.ts", "console.log('side');"), ("a.ts", "import './m';")]);
        let imports = index.get_imports(&paths[1]);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].kind, ImportKind::SideEffect);
    }

    #[test]
    fn resolves_index_files() {
        let (_dir, index, paths) = build_index(&[
            ("lib/index.ts", "export const x = 1;"),
            ("app.ts", "import { x } from './lib';"),
        ]);
        assert_eq!(index.get_usages(&paths[0], "x").len(), 1);
    }

    #[test]
    fn js_extension_specifier_resolves_to_ts_source() {
        // Regression for rbaumier/comply#968 — TypeScript ESM requires the
        // emitted `.js` extension in specifiers even when the source is `.ts`.
        let (_dir, index, paths) = build_index(&[
            ("core/checks.ts", "export const fmt = 1;"),
            ("locales/zh.ts", "import { fmt } from '../core/checks.js';"),
        ]);
        let imports = index.get_imports(&paths[1]);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source_path.as_ref(), Some(&paths[0]));
    }

    #[test]
    fn mjs_extension_specifier_resolves_to_mts_source() {
        let (_dir, index, paths) = build_index(&[
            ("util.mts", "export const u = 1;"),
            ("app.mts", "import { u } from './util.mjs';"),
        ]);
        let imports = index.get_imports(&paths[1]);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source_path.as_ref(), Some(&paths[0]));
    }

    #[test]
    fn exact_js_file_takes_priority_over_ts_counterpart() {
        let (_dir, index, paths) = build_index(&[
            ("foo.js", "export const f = 1;"),
            ("foo.ts", "export const f = 2;"),
            ("app.ts", "import { f } from './foo.js';"),
        ]);
        let imports = index.get_imports(&paths[2]);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source_path.as_ref(), Some(&paths[0]));
    }

    #[test]
    fn js_extension_specifier_without_any_source_stays_unresolved() {
        let (_dir, index, paths) =
            build_index(&[("app.ts", "import { x } from './missing.js';")]);
        let imports = index.get_imports(&paths[0]);
        assert_eq!(imports.len(), 1);
        assert!(imports[0].source_path.is_none());
    }

    #[test]
    fn bare_specifiers_stay_unresolved() {
        let (_dir, index, paths) = build_index(&[("a.ts", "import { useState } from 'react';")]);
        let imports = index.get_imports(&paths[0]);
        assert_eq!(imports.len(), 1);
        assert!(imports[0].source_path.is_none());
    }

    #[test]
    fn rust_external_crates_not_in_bare_specifiers() {
        // Regression: unresolved Rust `use` paths (external crates) must not
        // appear in bare_specifiers() — they are not npm packages.
        let (_dir, index, _paths) = build_index(&[(
            "lib.rs",
            "use turbopack_ecmascript_plugins::transform::StyledComponents;\nuse patterns::*;\n",
        )]);
        let bare = index.bare_specifiers();
        assert!(
            !bare.contains_key("turbopack_ecmascript_plugins"),
            "Rust external crate should not be in bare_specifiers"
        );
        assert!(
            !bare.contains_key("patterns"),
            "Rust module path should not be in bare_specifiers"
        );
    }

    #[test]
    fn reexport_emits_reexport_symbols() {
        let (_dir, index, paths) = build_index(&[
            ("a.ts", "export const a = 1;"),
            ("barrel.ts", "export { a } from './a';"),
        ]);
        let exports = index.get_exports(&paths[1]);
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].name, "a");
        assert_eq!(exports[0].kind, ExportKind::ReExport);
        assert_eq!(exports[0].reexport_source.as_deref(), Some("./a"));
    }

    #[test]
    fn star_reexport_emits_star_symbol() {
        let (_dir, index, paths) = build_index(&[
            ("a.ts", "export const a = 1;"),
            ("barrel.ts", "export * from './a';"),
        ]);
        let exports = index.get_exports(&paths[1]);
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].kind, ExportKind::StarReExport);
    }

    #[test]
    fn tracks_usages_from_multiple_importers() {
        let (_dir, index, paths) = build_index(&[
            ("m.ts", "export const foo = 1;"),
            ("a.ts", "import { foo } from './m';"),
            ("b.ts", "import { foo } from './m';"),
        ]);
        let usages = index.get_usages(&paths[0], "foo");
        assert_eq!(usages.len(), 2);
    }

    #[test]
    fn is_exported_anywhere_checks_all_files() {
        let (_dir, index, _paths) = build_index(&[
            ("a.ts", "export const alpha = 1;"),
            ("b.ts", "export const beta = 2;"),
        ]);
        assert!(index.is_exported_anywhere("alpha"));
        assert!(index.is_exported_anywhere("beta"));
        assert!(!index.is_exported_anywhere("gamma"));
    }

    #[test]
    fn empty_input_yields_empty_index() {
        let index = ImportIndex::build(&[]);
        assert!(index.get_exports(Path::new("nothing.ts")).is_empty());
        assert!(index.get_imports(Path::new("nothing.ts")).is_empty());
    }

    #[test]
    fn call_sites_distinguish_new_vs_plain_calls() {
        let (_dir, index, paths) = build_index(&[
            ("util.ts", "export function Widget() {}"),
            (
                "app.ts",
                "import { Widget } from './util';\nconst a = new Widget();\nconst b = Widget();\n",
            ),
        ]);
        let sites = index.get_call_sites(&paths[0], "Widget");
        assert_eq!(sites.len(), 2);
        assert_eq!(sites.iter().filter(|s| s.kind == CallKind::New).count(), 1);
        assert_eq!(sites.iter().filter(|s| s.kind == CallKind::Call).count(), 1);
        assert!(sites.iter().all(|s| s.path == paths[1]));
    }

    #[test]
    fn call_sites_translate_renamed_imports() {
        let (_dir, index, paths) = build_index(&[
            ("util.ts", "export function Widget() {}"),
            (
                "app.ts",
                "import { Widget as W } from './util';\nconst a = new W();\nconst b = W();\n",
            ),
        ]);
        // Keyed by the *exported* name, not the local rename.
        let sites = index.get_call_sites(&paths[0], "Widget");
        assert_eq!(sites.len(), 2);
        assert!(index.get_call_sites(&paths[0], "W").is_empty());
    }

    #[test]
    fn call_sites_skip_member_access() {
        let (_dir, index, paths) = build_index(&[
            ("util.ts", "export function Widget() {}"),
            (
                "app.ts",
                "import * as ns from './util';\nconst a = ns.Widget();\n",
            ),
        ]);
        // Namespace member-access is out of scope — no call-site recorded.
        assert!(index.get_call_sites(&paths[0], "Widget").is_empty());
    }

    #[test]
    fn vue_sfc_imports_and_exports_indexed() {
        let dir = TempDir::new().unwrap();
        let vue_path = dir.path().join("App.vue");
        fs::write(
            &vue_path,
            "<script setup>\nimport { ref } from 'vue';\nconst x = ref(0);\n</script>\n<template><div/></template>",
        )
        .unwrap();
        let source_file = SourceFile {
            path: vue_path.clone(),
            language: Language::Vue,
        };
        let index = ImportIndex::build(&[&source_file]);
        let canon = std::fs::canonicalize(&vue_path).unwrap();
        let exports = index.get_exports(&canon);
        assert!(
            exports.iter().any(|e| e.name == "default"),
            "Vue SFC should have implicit default export, got: {exports:?}"
        );
        let imports = index.get_imports(&canon);
        assert!(
            imports.iter().any(|i| i.specifier == "vue"),
            "Vue SFC should index imports from <script setup>, got: {imports:?}"
        );
    }

    #[test]
    fn vue_sfc_ts_file_can_import_vue() {
        let dir = TempDir::new().unwrap();
        let vue_path = dir.path().join("Comp.vue");
        fs::write(
            &vue_path,
            "<script setup>\nconst x = 1;\n</script>\n<template><div/></template>",
        )
        .unwrap();
        let ts_path = dir.path().join("main.ts");
        fs::write(&ts_path, "import Comp from './Comp.vue';").unwrap();
        let vue_file = SourceFile {
            path: vue_path.clone(),
            language: Language::Vue,
        };
        let ts_file = SourceFile {
            path: ts_path.clone(),
            language: Language::TypeScript,
        };
        let index = ImportIndex::build(&[&vue_file, &ts_file]);
        let canon_vue = std::fs::canonicalize(&vue_path).unwrap();
        let canon_ts = std::fs::canonicalize(&ts_path).unwrap();
        let imports = index.get_imports(&canon_ts);
        assert!(
            imports
                .iter()
                .any(|i| i.source_path.as_deref() == Some(canon_vue.as_path())),
            "TS file should resolve import of .vue file, got: {imports:?}"
        );
    }

    // ----------------------------- Rust tests -----------------------------

    #[test]
    fn rust_indexes_pub_items() {
        let (_dir, index, paths) = build_index(&[(
            "lib.rs",
            "pub fn foo() {}\n\
             pub struct Bar;\n\
             pub enum Baz { A }\n\
             pub trait Quux {}\n\
             pub type Alias = i32;\n\
             pub const K: i32 = 1;\n\
             pub static S: i32 = 2;\n\
             fn private() {}\n\
             struct Hidden;\n",
        )]);
        let names: Vec<&str> = index
            .get_exports(&paths[0])
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        for expected in ["foo", "Bar", "Baz", "Quux", "Alias", "K", "S"] {
            assert!(
                names.contains(&expected),
                "missing export {expected} — got {names:?}"
            );
        }
        assert!(!names.contains(&"private"));
        assert!(!names.contains(&"Hidden"));
    }

    #[test]
    fn rust_pub_mod_uses_module_kind() {
        let (_dir, index, paths) = build_index(&[
            ("lib.rs", "pub mod util;\n"),
            ("util.rs", "pub fn helper() {}\n"),
        ]);
        let exports = index.get_exports(&paths[0]);
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].name, "util");
        assert_eq!(exports[0].kind, ExportKind::Module);
    }

    #[test]
    fn rust_resolves_use_crate_path_and_links_usage() {
        let (_dir, index, paths) = build_index(&[
            ("lib.rs", "pub mod util;\npub use crate::util::helper;\n"),
            ("util.rs", "pub fn helper() {}\n"),
            (
                "app.rs",
                "use crate::util::helper;\nfn main() { helper(); }\n",
            ),
        ]);
        let util = &paths[1];
        let app = &paths[2];

        let imports = index.get_imports(app);
        let named: Vec<&ImportedSymbol> = imports
            .iter()
            .filter(|i| i.imported_name == "helper")
            .collect();
        assert_eq!(named.len(), 1, "imports: {imports:?}");
        assert_eq!(named[0].source_path.as_ref(), Some(util));

        let usages = index.get_usages(util, "helper");
        let importers: Vec<&Path> = usages.iter().map(|u| u.importer.as_path()).collect();
        assert!(importers.contains(&app.as_path()));
    }

    #[test]
    fn rust_pub_use_emits_reexport_symbol() {
        let (_dir, index, paths) = build_index(&[
            ("lib.rs", "pub mod util;\npub use crate::util::helper;\n"),
            ("util.rs", "pub fn helper() {}\n"),
        ]);
        let reexports: Vec<&ExportedSymbol> = index
            .get_exports(&paths[0])
            .iter()
            .filter(|e| e.kind == ExportKind::ReExport)
            .collect();
        assert_eq!(reexports.len(), 1);
        assert_eq!(reexports[0].name, "helper");
        assert_eq!(
            reexports[0].reexport_source.as_deref(),
            Some("crate::util::helper")
        );
    }

    #[test]
    fn rust_private_module_is_not_exported() {
        let (_dir, index, paths) = build_index(&[
            ("lib.rs", "mod util;\n"),
            ("util.rs", "pub fn helper() {}\n"),
        ]);
        let exports = index.get_exports(&paths[0]);
        assert!(
            exports.iter().all(|e| e.kind != ExportKind::Module),
            "unexpected module export: {exports:?}"
        );
    }

    #[test]
    fn rust_use_super_resolves_to_parent_module() {
        let (_dir, index, paths) = build_index(&[
            ("lib.rs", "pub mod a;\n"),
            ("a.rs", "pub mod b;\npub fn sibling() {}\n"),
            ("a/b.rs", "use super::sibling;\nfn call() { sibling(); }\n"),
        ]);
        let a_rs = &paths[1];
        let b_rs = &paths[2];
        let imports = index.get_imports(b_rs);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].imported_name, "sibling");
        assert_eq!(imports[0].source_path.as_ref(), Some(a_rs));
    }

    #[test]
    fn rust_use_aliased_import_translates_names() {
        let (_dir, index, paths) = build_index(&[
            ("lib.rs", "pub mod util;\n"),
            ("util.rs", "pub fn helper() {}\n"),
            ("app.rs", "use crate::util::helper as h;\n"),
        ]);
        let imports = index.get_imports(&paths[2]);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].imported_name, "helper");
        assert_eq!(imports[0].local_name, "h");
        assert_eq!(imports[0].source_path.as_ref(), Some(&paths[1]));

        assert_eq!(index.get_usages(&paths[1], "helper").len(), 1);
        assert!(index.get_usages(&paths[1], "h").is_empty());
    }

    #[test]
    fn rust_external_crate_uses_stay_unresolved() {
        let (_dir, index, paths) =
            build_index(&[("lib.rs", "use serde::Deserialize;\npub fn _noop() {}\n")]);
        let imports = index.get_imports(&paths[0]);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].imported_name, "Deserialize");
        assert!(imports[0].source_path.is_none());
    }

    #[test]
    fn rust_use_wildcard_is_namespace_kind() {
        let (_dir, index, paths) = build_index(&[
            ("lib.rs", "pub mod util;\n"),
            ("util.rs", "pub fn a() {}\npub fn b() {}\n"),
            ("app.rs", "use crate::util::*;\n"),
        ]);
        let imports = index.get_imports(&paths[2]);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].kind, ImportKind::Namespace);
        assert_eq!(imports[0].imported_name, "*");
    }

    #[test]
    fn tsconfig_paths_resolves_alias() {
        let dir = TempDir::new().unwrap();
        // Create tsconfig.json with path alias
        let tsconfig = r#"{
            "compilerOptions": {
                "baseUrl": ".",
                "paths": {
                    "@/*": ["src/*"]
                }
            }
        }"#;
        fs::write(dir.path().join("tsconfig.json"), tsconfig).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/utils.ts"),
            "export function helper() {}",
        )
        .unwrap();
        fs::write(
            dir.path().join("app.ts"),
            "import { helper } from '@/utils';",
        )
        .unwrap();

        let utils_path = dir.path().join("src/utils.ts");
        let app_path = dir.path().join("app.ts");

        let sources = [
            SourceFile {
                path: utils_path.clone(),
                language: Language::TypeScript,
            },
            SourceFile {
                path: app_path.clone(),
                language: Language::TypeScript,
            },
        ];
        let refs: Vec<&SourceFile> = sources.iter().collect();
        let index = ImportIndex::build(&refs);

        let utils_canon = fs::canonicalize(&utils_path).unwrap();
        let app_canon = fs::canonicalize(&app_path).unwrap();

        let imports = index.get_imports(&app_canon);
        assert_eq!(imports.len(), 1, "imports: {imports:?}");
        assert_eq!(imports[0].source_path.as_ref(), Some(&utils_canon));
    }

    // =======================================================================
    // Differential test: the oxc extractor must produce a byte-exact
    // `FileExtract` (same elements, same order, same line/column/offset) as the
    // tree-sitter path for TS/JS/TSX sources.
    // =======================================================================

    /// In-memory tree-sitter extraction mirroring `extract_for`'s TS/JS/TSX
    /// dispatch (no disk read, no canonicalization).
    fn extract_ts_treesitter(source: &str, lang: Language) -> FileExtract {
        let grammar: tree_sitter::Language = match lang {
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Language::TypeScript | Language::JavaScript => {
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
            }
            other => panic!("unexpected language {other:?}"),
        };
        let mut parser = Parser::new();
        parser.set_language(&grammar).unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();
        let bytes = source.as_bytes();

        let mut exports = Vec::new();
        let mut imports = Vec::new();
        let mut calls = Vec::new();
        let mut dynamic_dirs = Vec::new();
        walk_tree(&tree, |node| match node.kind() {
            "import_statement" => extract_import(node, bytes, &mut imports),
            "export_statement" => extract_export(node, bytes, &mut exports),
            "new_expression" => extract_call(node, bytes, CallKind::New, &mut calls),
            "call_expression" => {
                if node
                    .child_by_field_name("function")
                    .is_some_and(|c| c.kind() == "import")
                {
                    extract_dynamic_import(node, bytes, &mut imports, &mut dynamic_dirs);
                } else {
                    extract_require(node, bytes, &mut imports);
                    extract_call(node, bytes, CallKind::Call, &mut calls);
                }
            }
            _ => {}
        });
        FileExtract {
            exports,
            imports,
            calls,
            dynamic_dirs,
        }
    }

    /// Every distinct case the oxc extractor must match, one source string each.
    const DIFF_CASES: &[&str] = &[
        // --- imports ---
        "import foo from './m';",
        "import * as ns from './m';",
        "import { a, b } from './m';",
        "import { a as b } from './m';",
        "import { a, b as c, d } from './m';",
        "import './side-effect';",
        "import {} from './empty';",
        "import def, { a, b as c } from './m';",
        "import def, * as ns from './m';",
        "import React from 'react';",
        // type-only imports (statement-level + per-specifier)
        "import type { T } from './t';",
        "import type Foo from './t';",
        "import { type T, value } from './t';",
        "import { type A as B } from './t';",
        "import type * as NS from './t';",
        // dynamic import / require
        "const x = import('./dyn');",
        "async function f() { const m = await import('./dyn'); return m; }",
        "const y = require('./req');",
        "const z = require('not-relative');",
        "notRequire('./nope');",
        // template-literal specifiers must be ignored by both (not StringLiteral)
        "const t = import(`./tpl`);",
        "const r = require(`./tpl`);",
        // interpolated template-literal dynamic import: both extractors record
        // the static directory prefix into `dynamic_dirs`, not into `imports`.
        "const d = import(`./locales/${lang}.ts`);",
        "const e = import(`../../compositions/src/${scope}/${name}`);",
        // --- exports ---
        "export * from './m';",
        "export * as ns from './m';",
        "export { a, b } from './m';",
        "export { a as b } from './m';",
        "export { a, b as c };",
        "export {};",
        "export type { T } from './t';",
        "export type { T };",
        "export default 42;",
        "export default function hello() {}",
        "export default class Widget {}",
        "export default function (x, y) { return x + y; }",
        "export function fn(a, b, c) { return a; }",
        "export function* gen(x) { yield x; }",
        "export async function afn(p, q) { return p; }",
        "export function destructured({ a, b }, [c], ...rest) { return a; }",
        "export class Klass {}",
        "export abstract class AbstractK {}",
        "export const single = 1;",
        "export const a1 = 1, b1 = 2, c1 = 3;",
        "export let mutable = 0;",
        "export var legacy = 0;",
        "declare const obj: any; export const { signIn, signOut } = obj;",
        "declare const obj: any; export const { foo: bar } = obj;",
        "declare const obj: any; export const { a = 1, b } = obj;",
        "declare const arr: any; export const [first, second, ...others] = arr;",
        "declare const obj: any; export const { a: { b }, ...rest } = obj;",
        "declare const obj: any; export const [, skipped, { nested }] = obj;",
        "export type Alias = number;",
        "export interface Iface { x: number; }",
        "export enum Color { Red, Green }",
        // ambient `export declare …` — body-less but real named exports
        "export declare function ambientFn(name: string): void;",
        "export declare const ambientConst: number;",
        "export declare let ambientLet: number;",
        "export declare var ambientVar: number;",
        "export declare class AmbientClass {}",
        "export declare enum AmbientEnum { A, B }",
        "export declare namespace AmbientNs { const x: number; }",
        // namespace-scoped members are reachable only as `Ns.X`, never as a
        // module-level binding: both extractors record only the namespace name,
        // not its nested `export interface` / `export const` members. Two sibling
        // namespaces may each export an `X` without a module-level duplicate.
        "export namespace Ns { export interface X { a: number; } export const y = 1; }",
        "export namespace Outer { export interface X { a: number; }\n\
         export namespace Inner { export interface X { b: number; } } }",
        // overload signatures: body-less + implementation must yield ONE export
        "export function overloaded(a: string): void;\n\
         export function overloaded(a: number): void;\n\
         export function overloaded(a: any): void {}",
        // --- calls / new ---
        "f(a, b);",
        "f(a, obj.x, 42, ...spread);",
        "new Widget(config, handler);",
        "new Foo();",
        "ns.method(a);",
        "obj.prop.deep(x);",
        "f(g(h(a)), b);",
        "new Outer(new Inner(a));",
        "const c = compute(input);",
        // --- multi-byte / accents: offsets & columns must stay byte-based ---
        "const café = 1;\nimport { naïve } from './accentué';\nrender(café, naïve);",
        "// commentaire éàü\nnew Composé(arg);\nexport const ñ = 1;",
        "import { x } from './m';\n\n\nf(x);\nnew Y(z);",
        // 4-byte chars (emoji) inside strings before a call — byte offsets and
        // columns must stay byte-based, not char-based.
        "const s = '🚀🚀🚀'; launch(s, payload);\nnew Rocket(s);",
        "render('😀'); new Widget('x', '😀');",
        // --- a realistic mixed module ---
        "import React, { useState as useS, type FC } from 'react';\n\
         import * as utils from './utils';\n\
         import './styles.css';\n\
         export { helper } from './helpers';\n\
         export * from './all';\n\
         export * as everything from './everything';\n\
         export const value = 1, { destructured } = obj;\n\
         export default function App(props, ref) {\n\
           const [s, setS] = useState(0);\n\
           render(props);\n\
           return new Component(s);\n\
         }\n",
    ];

    #[test]
    fn oxc_matches_treesitter() {
        // Run every case under both grammars: `.tsx` (TSX) and `.ts`
        // (TypeScript). None of the cases use JSX, so both must agree, and the
        // oxc `SourceType` is selected from the path extension exactly as in
        // production (`source_type_for_path`).
        let variants: &[(Language, &str)] =
            &[(Language::Tsx, "diff.tsx"), (Language::TypeScript, "diff.ts")];
        for (lang, file) in variants {
            let path = Path::new(file);
            for (i, src) in DIFF_CASES.iter().enumerate() {
                let ts = extract_ts_treesitter(src, *lang);
                let oxc = extract_ts_oxc(src, path).expect("oxc extract");
                assert_eq!(
                    oxc, ts,
                    "case #{i} ({file}) diverged:\n--- source ---\n{src}\n\
                     --- oxc ---\n{oxc:#?}\n--- tree-sitter ---\n{ts:#?}"
                );
            }
        }
    }
}
