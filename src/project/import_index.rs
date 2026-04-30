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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
}

impl ImportIndex {
    /// Parse every TS/JS/TSX/Rust file in `files` and build the index. Vue
    /// files are ignored (Vue `<script>` blocks are not yet extracted).
    #[must_use]
    pub fn build(files: &[&SourceFile]) -> Self {
        // Per-file parse + extract runs in parallel; each worker gets its own
        // `Parser` because `tree_sitter::Parser` is !Sync. `map_init` is the
        // same pattern the engine already uses for rule dispatch.
        let per_file: Vec<(PathBuf, FileExtract)> = files
            .par_iter()
            .filter(|f| is_indexable(f.language))
            .map_init(Parser::new, |parser, file| extract_for(parser, file))
            .flatten()
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

        // First pass: stash exports, and partially-populate imports with their
        // raw specifiers resolved against disk (TS) or the module graph (Rust).
        for (path, mut extract) in per_file {
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
            exports.insert(path.clone(), extract.exports);
            imports.insert(path.clone(), extract.imports);
            file_calls.insert(path, extract.calls);
        }

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

        // Fourth pass: propagate re-exports. When barrel.ts does
        // `export { X } from './impl'`, usages on barrel flow to impl.
        propagate_reexports(&exports, &imports, &mut symbol_usages);

        // Fifth pass: collect bare specifiers (npm packages).
        let bare_specifiers = collect_bare_specifiers(&imports);

        // Sixth pass: Tarjan SCC for cycle detection.
        let cycles = compute_cycles(&imports);

        Self {
            exports,
            imports,
            symbol_usages,
            call_sites,
            bare_specifiers,
            cycles,
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
        let mut out = Vec::new();
        for (importer, imps) in &self.imports {
            if imps.iter().any(|i| i.source_path.as_deref() == Some(path)) {
                out.push(importer.as_path());
            }
        }
        out
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

    /// Bare specifiers (npm package names) collected from all imports.
    #[must_use]
    pub fn bare_specifiers(&self) -> &HashMap<String, BareSpecifierInfo> {
        &self.bare_specifiers
    }

    /// Files reachable from `roots` via import edges (BFS). Unreachable files
    /// in the indexed set are candidates for the `unused-file` rule.
    #[must_use]
    pub fn reachable_from(&self, roots: &[&Path]) -> std::collections::HashSet<PathBuf> {
        let mut visited = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<PathBuf> =
            roots.iter().map(|p| p.to_path_buf()).collect();
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
        }
        visited
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
/// any usage of `X` on barrel should count as a usage of `X` on impl.
fn propagate_reexports(
    exports: &HashMap<PathBuf, Vec<ExportedSymbol>>,
    imports: &HashMap<PathBuf, Vec<ImportedSymbol>>,
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

/// Extract bare specifier → package name mapping from all imports.
/// `@scope/pkg/path` → `@scope/pkg`, `lodash/fp` → `lodash`.
fn collect_bare_specifiers(
    imports: &HashMap<PathBuf, Vec<ImportedSymbol>>,
) -> HashMap<String, BareSpecifierInfo> {
    let mut result: HashMap<String, BareSpecifierInfo> = HashMap::new();
    for (file, imps) in imports {
        for imp in imps {
            if imp.source_path.is_some() || imp.specifier.starts_with('.') {
                continue;
            }
            let pkg = extract_package_name(&imp.specifier);
            if pkg.is_empty() || is_builtin_module(&pkg) {
                continue;
            }
            let entry = result.entry(pkg).or_insert(BareSpecifierInfo {
                type_only: true,
                importers: Vec::new(),
            });
            if !imp.is_type_only {
                entry.type_only = false;
            }
            if !entry.importers.contains(file) {
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
struct FileExtract {
    exports: Vec<ExportedSymbol>,
    imports: Vec<ImportedSymbol>,
    /// Raw call/new sites keyed by the local identifier at the call site.
    /// Cross-file linkage (local → exported name + source path) happens in
    /// `ImportIndex::build` using the file's import list.
    calls: Vec<LocalCall>,
}

/// A `new X(...)` / `X(...)` site captured during per-file extract. The
/// `local_name` is the identifier as written in this file; it is linked to an
/// exporting file + exported name later via the import list.
#[derive(Debug, Clone)]
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
            },
        ));
    }
    if matches!(file.language, Language::Vue) {
        return extract_vue(parser, &source, &file.path);
    }
    let grammar: tree_sitter::Language = match file.language {
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Language::TypeScript | Language::JavaScript => {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
        _ => return None,
    };
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(source.as_bytes(), None)?;

    let mut exports = Vec::new();
    let mut imports = Vec::new();
    let mut calls = Vec::new();
    walk_tree(&tree, |node| match node.kind() {
        "import_statement" => extract_import(node, source.as_bytes(), &mut imports),
        "export_statement" => extract_export(node, source.as_bytes(), &mut exports),
        "new_expression" => extract_call(node, source.as_bytes(), CallKind::New, &mut calls),
        "call_expression" => {
            if node
                .child_by_field_name("function")
                .is_some_and(|c| c.kind() == "import")
            {
                extract_dynamic_import(node, source.as_bytes(), &mut imports);
            } else {
                extract_require(node, source.as_bytes(), &mut imports);
                extract_call(node, source.as_bytes(), CallKind::Call, &mut calls);
            }
        }
        _ => {}
    });

    // Absolute-path canonicalization: rules compare paths by value, so two
    // different spellings of the same file (relative vs absolute) would miss
    // each other. Fall back to the given path if canonicalize fails.
    let canon = std::fs::canonicalize(&file.path).unwrap_or_else(|_| file.path.clone());
    Some((
        canon,
        FileExtract {
            exports,
            imports,
            calls,
        },
    ))
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
                    extract_dynamic_import(node, source_bytes, &mut imports);
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
        });
    }

    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Some((
        canon,
        FileExtract {
            exports,
            imports,
            calls,
        },
    ))
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
fn extract_dynamic_import(node: Node, source: &[u8], out: &mut Vec<ImportedSymbol>) {
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
    let line = node.start_position().row + 1;

    // `export * from './m'` / `export * as ns from './m'` — `export_clause`
    // may be absent; the wildcard is a `*` token child of export_statement.
    let has_star = node.children(&mut node.walk()).any(|c| c.kind() == "*");
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
            });
            return;
        }
        out.push(ExportedSymbol {
            name: "*".into(),
            kind: ExportKind::StarReExport,
            line,
            reexport_source: Some(src.clone()),
            params: Vec::new(),
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
            let name = match ids.as_slice() {
                [single] => text_of(*single, source),
                [_, aliased, ..] => text_of(*aliased, source),
                [] => continue,
            };
            out.push(ExportedSymbol {
                name,
                kind,
                line,
                reexport_source: source_str.clone(),
                params: Vec::new(),
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
        });
        return;
    }

    // `export function foo` / `export class Foo` / `export const foo = …` /
    // `export type Foo = …` / `export interface Foo` / `export enum Foo`
    for child in node.named_children(&mut node.walk()) {
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
                    });
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                // `const a = 1, b = 2` can export multiple names.
                let mut inner = child.walk();
                for decl in child.named_children(&mut inner) {
                    if decl.kind() != "variable_declarator" {
                        continue;
                    }
                    if let Some(id) = decl
                        .named_children(&mut decl.walk())
                        .find(|c| c.kind() == "identifier")
                    {
                        out.push(ExportedSymbol {
                            name: text_of(id, source),
                            kind: ExportKind::Named,
                            line,
                            reexport_source: None,
                            params: Vec::new(),
                        });
                    }
                }
            }
            "type_alias_declaration" | "interface_declaration" | "enum_declaration" => {
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
                    });
                }
            }
            _ => {}
        }
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
    let base_dir = importer.parent()?;
    probe_path(&base_dir.join(specifier), known)
}

/// Probe an absolute path (without or with extension) against the known set,
/// trying bare, each TS/JS extension, and `index.*` variants.
fn probe_path(raw: &Path, known: &std::collections::HashSet<PathBuf>) -> Option<PathBuf> {
    const EXTS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "mjs", "cts", "cjs", "vue"];

    let has_known_ext = raw
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| EXTS.contains(&ext));

    if has_known_ext {
        if let Ok(c) = std::fs::canonicalize(raw)
            && known.contains(&c)
        {
            return Some(c);
        }
        return None;
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
    None
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
                let Some(parent) = dir.parent() else { break };
                dir = parent;
            }
        }

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
        if let Some(e) = entry {
            if let Some(resolved) = Self::resolve_alias(specifier, &e.aliases, known) {
                return Some(resolved);
            }
        }

        // Fall through to oxc_resolver for node_modules / other resolution.
        let oxc = entry.map(|e| &e.oxc).or(self.fallback.as_ref())?;
        let importer_dir = importer.parent()?;
        let resolved = oxc.resolve(importer_dir, specifier).ok()?;
        let canonical = std::fs::canonicalize(resolved.path()).ok()?;
        known.contains(&canonical).then_some(canonical)
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
                if let Some(resolved) = probe_path(&expanded, known) {
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
    fn bare_specifiers_stay_unresolved() {
        let (_dir, index, paths) = build_index(&[("a.ts", "import { useState } from 'react';")]);
        let imports = index.get_imports(&paths[0]);
        assert_eq!(imports.len(), 1);
        assert!(imports[0].source_path.is_none());
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
}
