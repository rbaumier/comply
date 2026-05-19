// Infrastructure landing ahead of consumers: chantier #1 ships the
// ProjectCtx/FileCtx scaffolding, chantiers #2+ migrate rules onto it.
#![allow(dead_code)]

//! Project-level context loaded once per run.
//!
//! Operator consequence: rules that need `package.json` or `tsconfig.json`
//! read them through `ctx.project.nearest_*(path)` accessors instead of
//! re-parsing on every check. Lazy fields (Tailwind, Drizzle) only pay their
//! cost when a rule actually asks, and only once per run.
//!
//! How:
//! - `ProjectCtx::load(files, config)` detects the project root: nearest
//!   `package.json` above the common ancestor of `files`, else `.git`, else
//!   the common ancestor itself.
//! - Eager fields (root `package_json`, `tsconfig`, `framework`) load at
//!   startup.
//! - `nearest_*(path)` walk up from `path` to the closest matching manifest
//!   and cache the parsed result keyed by manifest directory — monorepo safe.
//! - Lazy fields use `OnceLock<Option<T>>`; parse failures cache as `None`
//!   (no retry within the run) and emit one stderr warning per field.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

use crate::config::Config;
use crate::files::SourceFile;
use crate::frameworks::FrameworkDef;

pub mod import_index;
pub mod k8s_index;
pub mod locale_index;

pub use import_index::ImportIndex;
pub use k8s_index::K8sIndex;
pub use locale_index::LocaleIndex;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModuleType {
    #[default]
    CommonJs,
    Module,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Framework {
    NextJs,
    TanStackStart,
    Vue,
    Nuxt,
    Remix,
    SvelteKit,
    #[default]
    Plain,
}

/// One parsed `package.json`. Dep sections are kept as sorted maps so
/// iteration order is stable across runs (helpful for rule output).
#[derive(Debug, Clone, Default)]
pub struct PackageJson {
    pub name: Option<String>,
    pub version: Option<String>,
    pub module_type: ModuleType,
    pub dependencies: BTreeMap<String, String>,
    pub dev_dependencies: BTreeMap<String, String>,
    pub peer_dependencies: BTreeMap<String, String>,
    pub optional_dependencies: BTreeMap<String, String>,
    pub engines: BTreeMap<String, String>,
    /// True if `browserslist` is present at any form (array, object, string).
    pub has_browserslist: bool,
    pub workspaces: Vec<String>,
    /// True if the package declares `main`, `exports`, or `module` — indicators
    /// that it's an npm library whose exports are consumed externally.
    pub is_library: bool,
}

impl PackageJson {
    pub fn parse(raw: &str) -> Option<Self> {
        let json: Value = serde_json::from_str(raw).ok()?;
        Some(PackageJson {
            name: json
                .get("name")
                .and_then(|node| node.as_str())
                .map(String::from),
            version: json
                .get("version")
                .and_then(|node| node.as_str())
                .map(String::from),
            module_type: match json.get("type").and_then(|node| node.as_str()) {
                Some("module") => ModuleType::Module,
                _ => ModuleType::CommonJs,
            },
            dependencies: parse_dep_map(&json, "dependencies"),
            dev_dependencies: parse_dep_map(&json, "devDependencies"),
            peer_dependencies: parse_dep_map(&json, "peerDependencies"),
            optional_dependencies: parse_dep_map(&json, "optionalDependencies"),
            engines: parse_dep_map(&json, "engines"),
            has_browserslist: json.get("browserslist").is_some(),
            is_library: json.get("main").is_some()
                || json.get("exports").is_some()
                || json.get("module").is_some(),
            workspaces: json
                .get("workspaces")
                .and_then(|node| node.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|node| node.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    /// Iterator over every declared package name across every dep section.
    /// Consumers looking up "is X declared anywhere?" use this — a `HashSet`
    /// view would force an allocation every call.
    pub fn all_deps(&self) -> impl Iterator<Item = &str> + '_ {
        self.dependencies
            .keys()
            .chain(self.dev_dependencies.keys())
            .chain(self.peer_dependencies.keys())
            .chain(self.optional_dependencies.keys())
            .map(String::as_str)
    }

    /// True if `name` appears in any dep section or in `engines`. `engines`
    /// keys name host runtimes (vscode, electron, node) that rules treat as
    /// importable specifiers — e.g. VSCode extensions declare
    /// `engines.vscode` and then `import vscode from 'vscode'`.
    pub fn has_dep_or_engine(&self, name: &str) -> bool {
        self.dependencies.contains_key(name)
            || self.dev_dependencies.contains_key(name)
            || self.peer_dependencies.contains_key(name)
            || self.optional_dependencies.contains_key(name)
            || self.engines.contains_key(name)
    }
}

fn parse_dep_map(json: &Value, section: &str) -> BTreeMap<String, String> {
    json.get(section)
        .and_then(|node| node.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(key, val)| (key.clone(), val.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Default)]
pub struct Tsconfig {
    pub paths: BTreeMap<String, Vec<String>>,
    pub base_url: Option<PathBuf>,
    pub module: Option<String>,
    pub module_resolution: Option<String>,
    pub strict: bool,
    pub jsx: Option<String>,
}

impl Tsconfig {
    pub fn parse(raw: &str) -> Option<Self> {
        let json: Value = parse_jsonc(raw)?;
        let co = json.get("compilerOptions");
        let paths = co
            .and_then(|x| x.get("paths"))
            .and_then(|x| x.as_object())
            .map(|o| {
                o.iter()
                    .map(|(k, val)| {
                        let list: Vec<String> = val
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        (k.clone(), list)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Tsconfig {
            paths,
            base_url: co
                .and_then(|x| x.get("baseUrl"))
                .and_then(|s| s.as_str())
                .map(PathBuf::from),
            module: co
                .and_then(|x| x.get("module"))
                .and_then(|s| s.as_str())
                .map(String::from),
            module_resolution: co
                .and_then(|x| x.get("moduleResolution"))
                .and_then(|s| s.as_str())
                .map(String::from),
            strict: co
                .and_then(|x| x.get("strict"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false),
            jsx: co
                .and_then(|x| x.get("jsx"))
                .and_then(|s| s.as_str())
                .map(String::from),
        })
    }

    /// Alias prefixes with any trailing `/*` stripped. Consumed by
    /// `no_implicit_deps` to decide whether a bare import matches a path
    /// alias and should be skipped.
    pub fn alias_prefixes(&self) -> Vec<String> {
        self.paths
            .keys()
            .map(|k| k.strip_suffix("/*").unwrap_or(k.as_str()).to_string())
            .collect()
    }

    /// Load `root/tsconfig.json` and recursively resolve any `extends` chain.
    /// Child `compilerOptions` win, but `paths` entries from parent tsconfigs
    /// are preserved when the child does not redeclare the same alias key —
    /// matches TypeScript's own merge semantics. Recursion is capped at 10
    /// levels to defend against pathological cycles.
    pub fn load(root: &Path) -> Option<Self> {
        load_tsconfig_file(&root.join("tsconfig.json"), 0)
    }
}

/// Read a tsconfig.json at `path`, follow its `extends` chain, and return the
/// merged result. Depth-tracked to bound recursion at 10 levels.
fn load_tsconfig_file(path: &Path, depth: u8) -> Option<Tsconfig> {
    if depth >= 10 {
        return None;
    }
    let raw = std::fs::read_to_string(path).ok()?;
    let json: Value = parse_jsonc(&raw)?;

    let mut merged = parse_tsconfig_value(&json);

    if let Some(extends) = json.get("extends").and_then(|v| v.as_str()) {
        let parent_path = resolve_extends(path, extends);
        if let Some(parent) = load_tsconfig_file(&parent_path, depth + 1) {
            merged = merge_tsconfig(parent, merged);
        }
    }

    Some(merged)
}

/// Resolve an `extends` reference relative to the directory containing the
/// referring tsconfig. Only relative-path strings are handled here; package
/// references (e.g. `"@tsconfig/node20/tsconfig.json"`) require node_modules
/// resolution which isn't wired up yet.
fn resolve_extends(referrer: &Path, extends: &str) -> PathBuf {
    let dir = referrer.parent().unwrap_or_else(|| Path::new("."));
    let mut candidate = dir.join(extends);
    if candidate.extension().is_none() && !candidate.is_file() {
        candidate.set_extension("json");
    }
    candidate
}

/// Parse a single tsconfig JSON value into a `Tsconfig`. Splitting this out
/// from `Tsconfig::parse` lets `load_tsconfig_file` reuse the field-extraction
/// logic without re-running `parse_jsonc`.
fn parse_tsconfig_value(json: &Value) -> Tsconfig {
    let co = json.get("compilerOptions");
    let paths = co
        .and_then(|x| x.get("paths"))
        .and_then(|x| x.as_object())
        .map(|o| {
            o.iter()
                .map(|(k, val)| {
                    let list: Vec<String> = val
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    (k.clone(), list)
                })
                .collect()
        })
        .unwrap_or_default();
    Tsconfig {
        paths,
        base_url: co
            .and_then(|x| x.get("baseUrl"))
            .and_then(|s| s.as_str())
            .map(PathBuf::from),
        module: co
            .and_then(|x| x.get("module"))
            .and_then(|s| s.as_str())
            .map(String::from),
        module_resolution: co
            .and_then(|x| x.get("moduleResolution"))
            .and_then(|s| s.as_str())
            .map(String::from),
        strict: co
            .and_then(|x| x.get("strict"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false),
        jsx: co
            .and_then(|x| x.get("jsx"))
            .and_then(|s| s.as_str())
            .map(String::from),
    }
}

/// Overlay `child` onto `parent`. Scalars (`base_url`, `module`,
/// `module_resolution`, `jsx`) are taken from the child when present; `paths`
/// are merged key-by-key so parent-only aliases survive. `strict` defaults to
/// false in `parse_tsconfig_value`, which means a child that omits the flag
/// inherits the parent's value here.
fn merge_tsconfig(parent: Tsconfig, child: Tsconfig) -> Tsconfig {
    let mut paths = parent.paths;
    for (k, v) in child.paths {
        paths.insert(k, v);
    }
    Tsconfig {
        paths,
        base_url: child.base_url.or(parent.base_url),
        module: child.module.or(parent.module),
        module_resolution: child.module_resolution.or(parent.module_resolution),
        strict: child.strict || parent.strict,
        jsx: child.jsx.or(parent.jsx),
    }
}

/// Parsed Tailwind theme. Populated statically from `@theme` CSS blocks (v4)
/// or object-literal `theme.extend.colors` in `tailwind.config.{ts,js}` (v3).
/// Stub today — future chantier.
#[derive(Debug, Default, Clone)]
pub struct TailwindTheme {
    pub colors: BTreeMap<String, String>,
    pub spacing: BTreeMap<String, String>,
}

/// Parsed Drizzle config. Stub today — future chantier.
#[derive(Debug, Default, Clone)]
pub struct DrizzleConfig {
    pub driver: Option<String>,
    pub schema_paths: Vec<PathBuf>,
}

#[derive(Debug, Default)]
pub struct ProjectCtx {
    pub project_root: Option<PathBuf>,
    pub workspace_roots: Vec<PathBuf>,
    pub package_json: Option<Arc<PackageJson>>,
    pub tsconfig: Option<Arc<Tsconfig>>,
    pub framework: Framework,
    pub detected_frameworks: Vec<&'static FrameworkDef>,

    // Per-manifest caches, keyed by the *directory* that contains the
    // manifest. Mutex over HashMap is sufficient: contention is low (same
    // manifest reused across sibling files hits the cache, so after the
    // first insert all readers take the lock briefly just to clone an Arc).
    package_json_cache: Mutex<HashMap<PathBuf, Arc<PackageJson>>>,
    tsconfig_cache: Mutex<HashMap<PathBuf, Arc<Tsconfig>>>,

    // Lazy project-wide fields. `OnceLock<Option<T>>` keeps the "init once,
    // cache None on failure, never retry" contract in a single primitive.
    tailwind_theme: OnceLock<Option<TailwindTheme>>,
    drizzle_config: OnceLock<Option<DrizzleConfig>>,

    // In diff modes the import index covers the full project but only a
    // subset of files is actually linted. Cross-file rules that emit
    // once-per-project use `anchor_path()` to pick a deterministic file
    // to attach their diagnostics to — that file must be among the linted
    // set, otherwise the diagnostic is never emitted.
    linted_paths: OnceLock<Vec<PathBuf>>,

    // Cross-file import/export index. Eagerly built by `load` when files are
    // known; empty (still queryable, returns no matches) for callers that
    // construct a `ProjectCtx` via `empty()` — e.g. the LSP server, where
    // single-file edits don't have a multi-file view yet.
    import_index: OnceLock<ImportIndex>,

    // Cross-file i18n locale index. Built lazily when first accessed.
    locale_index: OnceLock<LocaleIndex>,

    // Cross-file Kubernetes resource index. Eagerly built by `load`
    // when YAML files are in the input set; empty (still queryable)
    // otherwise — the same lazy-fallback pattern as `import_index`.
    k8s_index: OnceLock<K8sIndex>,

    // True when `project_root` contains a Cloudflare marker file
    // (`wrangler.toml`, `wrangler.jsonc`, `wrangler.json`, `.dev.vars`,
    // `_routes.json`). Lazily probed on first access — Cloudflare-only
    // rules need it to skip non-CF projects.
    cloudflare_target: OnceLock<bool>,
}

impl ProjectCtx {
    /// Empty instance — used by `default_static_project_ctx` and by the LSP
    /// path when no workspace context is available. `nearest_*` accessors
    /// still walk disk; only the eager root-level fields are absent.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Load once per run from the set of files being linted. Eagerly parses
    /// every TS/JS/TSX input to build `import_index` — cross-file rules are
    /// noisy/wrong without it, so we don't make that lookup lazy.
    pub fn load(files: &[&SourceFile], _config: &Config) -> Self {
        let root = detect_project_root(files);
        let pkg = root
            .as_ref()
            .and_then(|r| load_manifest_at(r, "package.json", PackageJson::parse))
            .map(Arc::new);
        let tsc = root.as_ref().and_then(|r| Tsconfig::load(r)).map(Arc::new);
        let framework = pkg.as_deref().map(detect_framework).unwrap_or_default();
        let detected_frameworks = pkg
            .as_deref()
            .map(|p| crate::frameworks::detect_frameworks(p, root.as_deref()))
            .unwrap_or_default();
        let workspace_roots = pkg
            .as_deref()
            .map(|p| resolve_workspace_roots(root.as_deref(), p))
            .unwrap_or_default();

        let mut ctx = ProjectCtx {
            project_root: root.clone(),
            workspace_roots,
            package_json: pkg.clone(),
            tsconfig: tsc.clone(),
            framework,
            detected_frameworks,
            ..Self::default()
        };

        // Seed the cache so rules that walk up from files under the root
        // don't re-read the same manifest they just loaded eagerly.
        if let (Some(r), Some(p)) = (root.as_ref(), pkg.as_ref()) {
            ctx.package_json_cache
                .get_mut()
                .unwrap()
                .insert(r.clone(), Arc::clone(p));
        }
        if let (Some(r), Some(t)) = (root.as_ref(), tsc.as_ref()) {
            ctx.tsconfig_cache
                .get_mut()
                .unwrap()
                .insert(r.clone(), Arc::clone(t));
        }

        // Eager cross-file index. Building here (instead of lazily on first
        // access) means the cost is paid once in the main thread before rule
        // dispatch fans out across rayon workers — rules see an already-built
        // `ImportIndex` and never contend on `OnceLock::get_or_init`.
        let index = ImportIndex::build(files);
        let _ = ctx.import_index.set(index);

        // Cross-file Kubernetes index. Same eager-build rationale as
        // `import_index`: pay the cost once before rule dispatch fans
        // out so consumers never contend on `OnceLock::get_or_init`.
        let k8s_idx = K8sIndex::build(files);
        let _ = ctx.k8s_index.set(k8s_idx);
        ctx
    }

    pub fn set_linted_paths(&self, paths: Vec<PathBuf>) {
        let _ = self.linted_paths.set(paths);
    }

    /// Deterministic anchor for once-per-project rules: the canonical
    /// smallest path among the files being linted. In full-scan mode this
    /// equals `indexed_paths().min()`; in diff mode it restricts to the
    /// changed files so the diagnostic is actually emitted.
    pub fn anchor_path(&self) -> Option<PathBuf> {
        if let Some(linted) = self.linted_paths.get() {
            linted.iter().min().cloned()
        } else {
            self.import_index().indexed_paths().min().map(Path::to_path_buf)
        }
    }

    /// Cross-file import/export index. Always returns a handle: when the
    /// index wasn't pre-built (e.g. `ProjectCtx::empty()` from the LSP),
    /// falls back to a shared empty index so callers never need to branch
    /// on availability — every accessor on an empty index returns an empty
    /// slice.
    pub fn import_index(&self) -> &ImportIndex {
        self.import_index.get_or_init(ImportIndex::default)
    }

    /// Access the locale index (i18n translation keys). Lazily initialized,
    /// returns empty index if not built.
    pub fn locale_index(&self) -> &LocaleIndex {
        self.locale_index.get_or_init(LocaleIndex::default)
    }

    /// Cross-file Kubernetes resource index. Always returns a handle:
    /// when the index wasn't pre-built (e.g. `ProjectCtx::empty()` from
    /// the LSP), falls back to a shared empty index so callers never
    /// need to branch on availability.
    pub fn k8s_index(&self) -> &K8sIndex {
        self.k8s_index.get_or_init(K8sIndex::default)
    }

    pub fn has_framework(&self, name: &str) -> bool {
        self.detected_frameworks.iter().any(|f| f.name == name)
    }

    /// True when the project root contains a Cloudflare marker file —
    /// `wrangler.toml`, `wrangler.jsonc`, `wrangler.json`, `.dev.vars`,
    /// or `_routes.json`. Used by Cloudflare-specific rules to skip
    /// projects that don't deploy to Workers / Pages. Result is cached
    /// for the lifetime of the run.
    pub fn is_cloudflare_target(&self) -> bool {
        let Some(root) = self.project_root.as_deref() else {
            return false;
        };
        *self.cloudflare_target.get_or_init(|| {
            const MARKERS: &[&str] = &[
                "wrangler.toml",
                "wrangler.jsonc",
                "wrangler.json",
                ".dev.vars",
                "_routes.json",
            ];
            MARKERS.iter().any(|name| root.join(name).metadata().is_ok())
        })
    }

    pub fn framework_entry_dirs(&self) -> impl Iterator<Item = &str> {
        self.detected_frameworks
            .iter()
            .flat_map(|f| f.entry_points.dirs.iter().map(String::as_str))
    }

    pub fn framework_entry_files(&self) -> impl Iterator<Item = &str> {
        self.detected_frameworks
            .iter()
            .flat_map(|f| f.entry_points.files.iter().map(String::as_str))
    }

    pub fn framework_entry_file_suffixes(&self) -> impl Iterator<Item = &str> {
        self.detected_frameworks
            .iter()
            .flat_map(|fw| fw.entry_points.file_suffixes.iter())
            .map(|s| s.as_str())
    }

    pub fn framework_root_files(&self) -> impl Iterator<Item = &str> {
        self.detected_frameworks
            .iter()
            .flat_map(|f| f.entry_points.root_files.iter().map(String::as_str))
    }

    pub fn framework_magic_exports(&self) -> impl Iterator<Item = &str> {
        self.detected_frameworks
            .iter()
            .flat_map(|f| f.magic_exports.names.iter().map(String::as_str))
    }

    pub fn framework_tooling_deps(&self) -> impl Iterator<Item = &str> {
        self.detected_frameworks
            .iter()
            .flat_map(|f| f.tooling_deps.names.iter().map(String::as_str))
    }

    #[cfg(test)]
    #[must_use]
    pub fn for_test_with_framework(name: &str) -> Self {
        let mut ctx = ProjectCtx::default();
        if let Some(fw) = crate::frameworks::get_framework(name) {
            ctx.detected_frameworks = vec![fw];
        }
        ctx
    }

    /// Test-only constructor that seeds `import_index` from an arbitrary set
    /// of `SourceFile`s. Lets cross-file rule tests exercise the index
    /// without spinning up a full `load`.
    #[cfg(test)]
    #[must_use]
    pub fn for_test_with_files(files: &[&SourceFile]) -> Self {
        let ctx = ProjectCtx::default();
        let index = ImportIndex::build(files);
        let _ = ctx.import_index.set(index);
        let k8s_index = K8sIndex::build(files);
        let _ = ctx.k8s_index.set(k8s_index);
        ctx
    }

    /// Walk up from `path` to the nearest `package.json`, cache the parsed
    /// result by manifest directory. Returns the same `Arc` on repeated
    /// lookups against any file under the same manifest.
    pub fn nearest_package_json(&self, path: &Path) -> Option<Arc<PackageJson>> {
        nearest(
            &self.package_json_cache,
            path,
            "package.json",
            PackageJson::parse,
        )
    }

    /// Walk up from `path` to the nearest `tsconfig.json`, cache by manifest
    /// directory.
    pub fn nearest_tsconfig(&self, path: &Path) -> Option<Arc<Tsconfig>> {
        nearest(&self.tsconfig_cache, path, "tsconfig.json", Tsconfig::parse)
    }

    /// Lazily-loaded Tailwind theme. Stub returns `None`; future chantier
    /// populates this from CSS `@theme` blocks and static v3 TS configs.
    pub fn tailwind_theme(&self) -> Option<&TailwindTheme> {
        self.tailwind_theme.get_or_init(|| None).as_ref()
    }

    /// Lazily-loaded Drizzle config. Stub — see `tailwind_theme`.
    pub fn drizzle_config(&self) -> Option<&DrizzleConfig> {
        self.drizzle_config.get_or_init(|| None).as_ref()
    }

    /// Package names from all workspace members. Used by `unlisted-dependency`
    /// to recognize cross-workspace imports as valid.
    pub fn workspace_package_names(&self) -> Vec<String> {
        self.workspace_roots
            .iter()
            .filter_map(|root| {
                let raw = std::fs::read_to_string(root.join("package.json")).ok()?;
                let pkg = PackageJson::parse(&raw)?;
                pkg.name
            })
            .collect()
    }
}

/// Resolve workspace glob patterns to actual package directories.
/// Returns the list of workspace root directories found on disk.
fn resolve_workspace_roots(project_root: Option<&Path>, pkg: &PackageJson) -> Vec<PathBuf> {
    let Some(root) = project_root else {
        return Vec::new();
    };
    if pkg.workspaces.is_empty() {
        return Vec::new();
    }

    let mut roots = Vec::new();
    for pattern in &pkg.workspaces {
        // Simple glob: "packages/*" -> list directories matching the pattern
        let base = root.join(pattern.trim_end_matches('*').trim_end_matches('/'));
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("package.json").exists() {
                    roots.push(path);
                }
            }
        }
        // Also handle exact paths (no glob)
        if !pattern.contains('*') {
            let exact = root.join(pattern);
            if exact.is_dir() && exact.join("package.json").exists() {
                roots.push(exact);
            }
        }
    }
    roots
}

/// Walk up from `path` to the nearest `filename`, returning a cached parse.
/// Cache miss: read + parse + insert at the manifest directory. Cache hit:
/// clone the `Arc` under the lock.
fn nearest<T>(
    cache: &Mutex<HashMap<PathBuf, Arc<T>>>,
    path: &Path,
    filename: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Option<Arc<T>> {
    let start_dir = path.parent()?;

    // Fast path: any cached ancestor.
    {
        let map = cache.lock().ok()?;
        let mut cur = Some(start_dir);
        while let Some(dir) = cur {
            if let Some(hit) = map.get(dir) {
                return Some(Arc::clone(hit));
            }
            cur = dir.parent();
        }
    }

    // Slow path: walk disk.
    let (manifest_dir, parsed) = walk_up_for(start_dir, filename, parse)?;
    let arc = Arc::new(parsed);
    if let Ok(mut map) = cache.lock() {
        map.entry(manifest_dir).or_insert_with(|| Arc::clone(&arc));
    }
    Some(arc)
}

fn detect_project_root(files: &[&SourceFile]) -> Option<PathBuf> {
    let start = common_ancestor(files)?;
    if let Some(dir) = walk_up_finding(&start, "package.json") {
        return Some(dir);
    }
    if let Some(dir) = walk_up_finding(&start, ".git") {
        return Some(dir);
    }
    Some(start)
}

fn common_ancestor(files: &[&SourceFile]) -> Option<PathBuf> {
    let mut iter = files.iter().filter_map(|f| f.path.parent());
    let first = iter.next()?.to_path_buf();
    let mut common = first;
    for p in iter {
        while !p.starts_with(&common) {
            let parent = common.parent()?;
            common = parent.to_path_buf();
        }
    }
    Some(common)
}

pub(crate) fn walk_up_finding(start: &Path, target: &str) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        if dir.join(target).exists() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

fn walk_up_for<T>(
    start: &Path,
    filename: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Option<(PathBuf, T)> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        let candidate = dir.join(filename);
        if candidate.is_file() {
            let raw = std::fs::read_to_string(&candidate).ok()?;
            match parse(&raw) {
                Some(parsed) => return Some((dir.to_path_buf(), parsed)),
                None => {
                    eprintln!("comply: ignoring malformed {}", candidate.display());
                    return None;
                }
            }
        }
        cur = dir.parent();
    }
    None
}

fn load_manifest_at<T>(
    root: &Path,
    filename: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Option<T> {
    let path = root.join(filename);
    let raw = std::fs::read_to_string(&path).ok()?;
    let parsed = parse(&raw);
    if parsed.is_none() {
        eprintln!("comply: ignoring malformed {}", path.display());
    }
    parsed
}

fn detect_framework(pkg: &PackageJson) -> Framework {
    let has = |name: &str| pkg.all_deps().any(|k| k == name);
    if has("nuxt") {
        Framework::Nuxt
    } else if has("next") {
        Framework::NextJs
    } else if has("@tanstack/start") || has("@tanstack/react-start") {
        Framework::TanStackStart
    } else if has("@remix-run/react") {
        Framework::Remix
    } else if has("@sveltejs/kit") {
        Framework::SvelteKit
    } else if has("vue") {
        Framework::Vue
    } else {
        Framework::Plain
    }
}

/// Strip `//`-to-end-of-line comments, leaving `//` inside string literals
/// alone. tsconfig.json is jsonc-ish; serde_json rejects line comments so we
/// normalise first.
fn parse_jsonc(raw: &str) -> Option<Value> {
    let stripped = json_comments::StripComments::new(raw.as_bytes());
    serde_json::from_reader(stripped).ok()
}

/// Process-wide default `ProjectCtx` used by `CheckCtx::for_test`. Production
/// code always threads a real `ProjectCtx` through from the engine.
#[cfg(test)]
pub(crate) fn default_static_project_ctx() -> &'static ProjectCtx {
    static DEFAULT: OnceLock<ProjectCtx> = OnceLock::new();
    DEFAULT.get_or_init(ProjectCtx::empty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn package_json_parses_dep_sections() {
        let pkg = PackageJson::parse(
            r#"{
            "name":"a","version":"1.0.0","type":"module",
            "dependencies":{"react":"^19"},
            "devDependencies":{"vitest":"^1"},
            "engines":{"node":"22"}
        }"#,
        )
        .unwrap();
        assert_eq!(pkg.name.as_deref(), Some("a"));
        assert_eq!(pkg.module_type, ModuleType::Module);
        assert!(pkg.dependencies.contains_key("react"));
        assert!(pkg.dev_dependencies.contains_key("vitest"));
        assert!(pkg.engines.contains_key("node"));
    }

    #[test]
    fn has_dep_or_engine_covers_every_section() {
        let pkg = PackageJson::parse(
            r#"{"optionalDependencies":{"fsevents":"^2"},"engines":{"vscode":"^1"}}"#,
        )
        .unwrap();
        assert!(pkg.has_dep_or_engine("fsevents"));
        assert!(pkg.has_dep_or_engine("vscode"));
        assert!(!pkg.has_dep_or_engine("react"));
    }

    #[test]
    fn tsconfig_parses_paths_with_line_comments() {
        let ts = Tsconfig::parse(
            "{\n  // hello\n  \"compilerOptions\":{\"paths\":{\"~/*\":[\"./src/*\"]}}\n}",
        )
        .unwrap();
        assert!(ts.paths.contains_key("~/*"));
        assert_eq!(ts.alias_prefixes(), vec!["~".to_string()]);
    }

    #[test]
    fn detect_framework_next() {
        let mut pkg = PackageJson::default();
        pkg.dependencies.insert("next".into(), "^14".into());
        assert_eq!(detect_framework(&pkg), Framework::NextJs);
    }

    #[test]
    fn detect_framework_nuxt_beats_vue() {
        let mut pkg = PackageJson::default();
        pkg.dependencies.insert("nuxt".into(), "^3".into());
        pkg.dependencies.insert("vue".into(), "^3".into());
        assert_eq!(detect_framework(&pkg), Framework::Nuxt);
    }

    #[test]
    fn empty_ctx_has_no_project_data() {
        let ctx = ProjectCtx::empty();
        assert!(ctx.package_json.is_none());
        assert!(ctx.tsconfig.is_none());
        assert_eq!(ctx.framework, Framework::Plain);
    }

    #[test]
    fn nearest_package_json_walks_up_and_caches() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"x"}"#).unwrap();
        let nested = dir.path().join("src").join("deep");
        std::fs::create_dir_all(&nested).unwrap();

        let ctx = ProjectCtx::empty();
        let first = ctx.nearest_package_json(&nested.join("t.ts")).unwrap();
        let second = ctx.nearest_package_json(&nested.join("other.ts")).unwrap();
        assert!(
            Arc::ptr_eq(&first, &second),
            "sibling files should share the same cached Arc"
        );
        assert_eq!(first.name.as_deref(), Some("x"));
    }

    #[test]
    fn nearest_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        let ctx = ProjectCtx::empty();
        assert!(ctx.nearest_package_json(&dir.path().join("t.ts")).is_none());
    }

    #[test]
    fn malformed_package_json_returns_none() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), "{ not json").unwrap();
        let ctx = ProjectCtx::empty();
        assert!(ctx.nearest_package_json(&dir.path().join("t.ts")).is_none());
    }

    #[test]
    fn resolves_workspace_packages() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        let foo = dir.path().join("packages").join("foo");
        let bar = dir.path().join("packages").join("bar");
        std::fs::create_dir_all(&foo).unwrap();
        std::fs::create_dir_all(&bar).unwrap();
        std::fs::write(foo.join("package.json"), r#"{"name":"@scope/foo"}"#).unwrap();
        std::fs::write(bar.join("package.json"), r#"{"name":"@scope/bar"}"#).unwrap();

        let pkg = PackageJson::parse(r#"{"name":"root","workspaces":["packages/*"]}"#).unwrap();
        let roots = resolve_workspace_roots(Some(dir.path()), &pkg);
        assert_eq!(roots.len(), 2);

        let ctx = ProjectCtx {
            workspace_roots: roots,
            ..ProjectCtx::default()
        };
        let mut names = ctx.workspace_package_names();
        names.sort();
        assert_eq!(
            names,
            vec!["@scope/bar".to_string(), "@scope/foo".to_string()]
        );
    }

    #[test]
    fn empty_workspaces_returns_empty() {
        let dir = TempDir::new().unwrap();
        let pkg = PackageJson::parse(r#"{"name":"root"}"#).unwrap();
        let roots = resolve_workspace_roots(Some(dir.path()), &pkg);
        assert!(roots.is_empty());

        let ctx = ProjectCtx::default();
        assert!(ctx.workspace_package_names().is_empty());
    }

    #[test]
    fn extends_merges_paths() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("tsconfig.base.json"),
            r#"{"compilerOptions":{"paths":{"@base/*":["./base/*"]}}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"extends":"./tsconfig.base.json","compilerOptions":{"paths":{"@app/*":["./app/*"]}}}"#,
        )
        .unwrap();
        let ts = Tsconfig::load(dir.path()).unwrap();
        assert!(ts.paths.contains_key("@base/*"));
        assert!(ts.paths.contains_key("@app/*"));
    }

    #[test]
    fn child_overrides_parent_path() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("tsconfig.base.json"),
            r#"{"compilerOptions":{"paths":{"@/*":["./parent/*"]}}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"extends":"./tsconfig.base.json","compilerOptions":{"paths":{"@/*":["./child/*"]}}}"#,
        )
        .unwrap();
        let ts = Tsconfig::load(dir.path()).unwrap();
        assert_eq!(ts.paths.get("@/*").unwrap(), &vec!["./child/*".to_string()]);
    }

    #[test]
    fn extends_resolves_relative() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("configs");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            sub.join("base.json"),
            r#"{"compilerOptions":{"paths":{"@base/*":["./base/*"]},"strict":true}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"extends":"./configs/base.json","compilerOptions":{"paths":{"@app/*":["./app/*"]}}}"#,
        )
        .unwrap();
        let ts = Tsconfig::load(dir.path()).unwrap();
        assert!(ts.paths.contains_key("@base/*"));
        assert!(ts.paths.contains_key("@app/*"));
        assert!(ts.strict);
    }

    #[test]
    fn no_extends_works_as_before() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"paths":{"~/*":["./src/*"]},"jsx":"preserve"}}"#,
        )
        .unwrap();
        let ts = Tsconfig::load(dir.path()).unwrap();
        assert!(ts.paths.contains_key("~/*"));
        assert_eq!(ts.jsx.as_deref(), Some("preserve"));
    }
}
