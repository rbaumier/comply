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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

use crate::config::Config;
use crate::files::SourceFile;
use crate::frameworks::FrameworkDef;

pub mod eslint_ignore;
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
    /// Keys of the top-level `imports` field — Node.js subpath imports. Each is
    /// a `#`-prefixed self-referencing alias (e.g. `#import-plugin`, `#dep/*`)
    /// resolved to an internal file at runtime, never an npm package.
    pub subpath_imports: BTreeSet<String>,
    /// True if `browserslist` is present at any form (array, object, string).
    pub has_browserslist: bool,
    pub workspaces: Vec<String>,
    /// True if the package declares `main`, `exports`, or `module` — indicators
    /// that it's an npm library whose exports are consumed externally.
    pub is_library: bool,
    /// True if the package declares a `bin` field — it's a CLI-tool package whose
    /// `src/**` implements one or more published binaries. Sibling packages
    /// consume it by invoking the binary, never by ES-importing its modules, and
    /// the tool's own command framework wires up internal modules dynamically, so
    /// their exports have no static importer.
    pub has_bin: bool,
    /// Relative paths of source files that appear as CLI entry points in the
    /// `scripts` field (e.g. `"seed:dev": "bun run src/db/seed/dev.ts"`).
    /// Stored with forward slashes and without a leading `./`.
    pub script_entry_files: Vec<String>,
    /// Test-runner binaries invoked as a command by any `scripts` entry
    /// (e.g. `vitest` from `"test": "vitest run"`). A dependency listed in
    /// `devDependencies` alone does not appear here — only binaries actually
    /// run by a script — so consumers can tell "uses X as its runner" apart
    /// from "ships an integration/plugin for X".
    pub script_test_runners: BTreeSet<String>,
    /// Command heads (binary names) invoked by any `scripts` entry — the first
    /// token of every `&&`/`|`/`;`-separated segment, with any path or `.bin`
    /// prefix stripped (e.g. `changeset` from `"release": "changeset publish"`).
    /// Lets a consumer recognize a CLI-runner package whose binary a script
    /// runs even though no source file ES-imports the package.
    pub script_command_heads: BTreeSet<String>,
    /// Relative paths this package declares as its own entry point: the `main`
    /// value, the `exports` `.` target(s), and the `browser`/`react-native`
    /// substitute targets (the browser/native build bundlers swap in). Stored
    /// manifest-dir-relative, forward-slash, no leading `./`, so a consumer can
    /// join them onto the manifest directory and compare against a file path.
    pub entry_files: BTreeSet<String>,
    /// True when every published entry (`main`/`module`/`exports`/`browser`/
    /// `react-native`) lives outside a top-level `src/` directory and at least
    /// one such entry exists. Marks `src/` as build *input* whose contents are
    /// compiled away into the shipped artifact, so a devDependency imported from
    /// `src/` is bundled at build time, not a runtime dependency.
    pub entries_outside_src: bool,
    /// File stems (basename without extension) of every published entry across
    /// all `exports` subpaths plus `main`/`module` — e.g. `framer-motion`'s
    /// `.`→`dist/es/index.mjs`, `./dom`→`dist/es/dom.mjs` yield `{index, dom}`.
    /// Published entries point at built `dist/` artifacts while the source
    /// barrels (`src/index.ts`, `src/dom.ts`) carry the same stem, so stems
    /// identify which source files are distinct public entry points of a
    /// multi-entry package.
    pub export_entry_stems: BTreeSet<String>,
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
            subpath_imports: json
                .get("imports")
                .and_then(|node| node.as_object())
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default(),
            has_browserslist: json.get("browserslist").is_some(),
            is_library: json.get("main").is_some()
                || json.get("exports").is_some()
                || json.get("module").is_some(),
            has_bin: json.get("bin").is_some(),
            workspaces: parse_workspaces(&json),
            script_entry_files: json
                .get("scripts")
                .and_then(|node| node.as_object())
                .map(|obj| {
                    obj.values()
                        .filter_map(|v| v.as_str())
                        .flat_map(extract_script_entry_files)
                        .collect()
                })
                .unwrap_or_default(),
            script_test_runners: json
                .get("scripts")
                .and_then(|node| node.as_object())
                .map(|obj| {
                    obj.values()
                        .filter_map(|v| v.as_str())
                        .flat_map(extract_script_test_runners)
                        .collect()
                })
                .unwrap_or_default(),
            script_command_heads: json
                .get("scripts")
                .and_then(|node| node.as_object())
                .map(|obj| {
                    obj.values()
                        .filter_map(|v| v.as_str())
                        .flat_map(extract_script_command_heads)
                        .collect()
                })
                .unwrap_or_default(),
            entry_files: collect_entry_files(&json),
            entries_outside_src: entries_outside_src(&json),
            export_entry_stems: collect_export_entry_stems(&json),
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

    /// Minimum supported Node.js version (`major`, `minor`) parsed from the
    /// `engines.node` range, or `None` when no `node` constraint is declared.
    ///
    /// A range may list `||`-separated alternatives (e.g. `>=18.18 || >=20.9`);
    /// the smallest alternative wins, since the package must run on every
    /// version it permits. The minor component is needed by callers that gate on
    /// sub-major thresholds (Node features backported within a major line, such
    /// as `import.meta.dirname` landing in 20.11 and 21.2).
    pub fn min_node_version(&self) -> Option<(u32, u32)> {
        let spec = self.engines.get("node")?;
        spec.split("||")
            .filter_map(parse_node_range_min)
            .min()
    }

    /// True if `spec` is a Node.js subpath import declared in this manifest's
    /// `imports` field. Matches an exact key (`#import-plugin`) or a wildcard
    /// pattern (`#dep/*` covers `#dep/anything`). `spec` is the bare-specifier
    /// package head, so a subpath like `#dep/db` arrives reduced to `#dep`; a
    /// `#dep/*` key (prefix `#dep/`) therefore matches it on the trimmed prefix.
    /// These `#`-aliases resolve to internal files, never an npm dependency.
    pub fn declares_subpath_import(&self, spec: &str) -> bool {
        self.subpath_imports.iter().any(|key| {
            if key == spec {
                return true;
            }
            match key.strip_suffix('*') {
                Some(prefix) => {
                    let prefix = prefix.strip_suffix('/').unwrap_or(prefix);
                    spec == prefix || spec.starts_with(prefix)
                }
                None => false,
            }
        })
    }

    /// True if any `scripts` entry runs `name` (e.g. `vitest`) as a command.
    /// Evidence the package uses `name` as its test runner, as opposed to
    /// merely listing it in `devDependencies` to exercise an integration.
    pub fn scripts_invoke_test_runner(&self, name: &str) -> bool {
        self.script_test_runners.contains(name)
    }

    /// True if dependency `name` is a CLI-runner package whose provided binary
    /// is invoked by a `scripts` command. CLI-runner packages (`@scope/cli`,
    /// `*-cli`, `*-bin`) ship a binary that scripts run (`changeset publish`,
    /// `manypkg check`) and are never ES-imported, so the import index sees no
    /// usage. There is no node_modules access to read the package's own `bin`
    /// field, so candidate binary names are derived from the package name and
    /// matched against the command heads seen in `scripts`.
    pub fn scripts_invoke_dep_binary(&self, name: &str) -> bool {
        cli_runner_binary_candidates(name)
            .iter()
            .any(|candidate| self.script_command_heads.contains(candidate))
    }

    /// True if `name` is this package's own `name` field — a Node.js
    /// self-reference. A package never lists itself as a dependency, yet it may
    /// import from itself by its published name (`import x from "preact"` or a
    /// subpath `import x from "preact/hooks"`), which the toolchain resolves to
    /// the package's own source. `name` is the bare-specifier package head, so a
    /// subpath like `preact/hooks` arrives reduced to `preact`.
    pub fn is_self_name(&self, name: &str) -> bool {
        self.name.as_deref() == Some(name)
    }
}

/// Parse the lower bound (`major`, `minor`) of a single semver range alternative.
///
/// Reads the first version literal in the range — the lower bound of `>=`, `^`,
/// `~`, a bare `20.11.0`, or `18.x` style specs — and returns its major and
/// minor (defaulting the minor to `0` when absent, e.g. `>=18`). Returns `None`
/// when the range contains no leading numeric version (`*`, `latest`, garbage).
fn parse_node_range_min(range: &str) -> Option<(u32, u32)> {
    let bytes = range.as_bytes();
    let mut i = 0;
    while i < bytes.len() && !bytes[i].is_ascii_digit() {
        i += 1;
    }
    let major = read_uint(bytes, &mut i)?;
    let minor = if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        read_uint(bytes, &mut i).unwrap_or(0)
    } else {
        0
    };
    Some((major, minor))
}

/// Read a run of ASCII digits at `*i` into a `u32`, advancing `*i` past them.
/// Returns `None` when no digit is present at the cursor.
fn read_uint(bytes: &[u8], i: &mut usize) -> Option<u32> {
    let start = *i;
    while *i < bytes.len() && bytes[*i].is_ascii_digit() {
        *i += 1;
    }
    if *i == start {
        return None;
    }
    std::str::from_utf8(&bytes[start..*i]).ok()?.parse().ok()
}

/// Extract source-file paths from a package.json script command value.
///
/// Splits the command by whitespace and keeps tokens that end with a known
/// source extension (`.ts`, `.tsx`, `.mts`, `.js`, `.mjs`, `.cjs`). Leading
/// `./` is stripped so callers can compare against project-root-relative paths.
fn extract_script_entry_files(cmd: &str) -> Vec<String> {
    const SOURCE_EXTS: &[&str] = &[".ts", ".tsx", ".mts", ".js", ".mjs", ".cjs"];
    cmd.split_whitespace()
        .filter(|token| SOURCE_EXTS.iter().any(|ext| token.ends_with(ext)))
        .map(|token| token.strip_prefix("./").unwrap_or(token).to_string())
        .collect()
}

/// Test-runner binaries (`vitest`, `jest`) invoked as a command by a script.
///
/// Tokenizes the command on shell separators and whitespace, strips any path /
/// `.bin` prefix from each token, and keeps tokens whose basename names a known
/// runner — so `vitest run`, `npx vitest`, and `node_modules/.bin/jest` all
/// count, while a bare `vitest.config.ts` path or a `--reporter=vitest` flag do
/// not (they are not command heads of the form the runner binary takes).
fn extract_script_test_runners(cmd: &str) -> Vec<String> {
    const RUNNERS: &[&str] = &["vitest", "jest"];
    cmd.split(|c: char| c.is_whitespace() || matches!(c, '&' | '|' | ';' | '(' | ')'))
        .filter(|token| !token.is_empty())
        .map(|token| token.rsplit('/').next().unwrap_or(token))
        .filter(|name| RUNNERS.contains(name))
        .map(str::to_string)
        .collect()
}

/// Command heads (binary names) invoked by a package.json script command.
///
/// Splits the command on shell separators (`&&`, `||`, `;`, `|`, subshell
/// parens) into segments, takes the first whitespace-delimited token of each
/// segment, and strips any path / `.bin` prefix to its basename — so `changeset
/// publish`, `pnpm -r build && manypkg check`, and `node_modules/.bin/eslint`
/// yield `changeset`, `{pnpm, manypkg}`, and `eslint`. Flag-leading segments
/// (a token starting with `-`) and empty segments name no binary and are
/// dropped.
fn extract_script_command_heads(cmd: &str) -> Vec<String> {
    cmd.split(|c: char| matches!(c, '&' | '|' | ';' | '(' | ')'))
        .filter_map(|segment| segment.split_whitespace().next())
        .filter(|head| !head.starts_with('-'))
        .map(|head| head.rsplit('/').next().unwrap_or(head))
        .filter(|head| !head.is_empty())
        .map(str::to_string)
        .collect()
}

/// Candidate binary names a CLI-runner package `name` might provide. Empty for
/// a package that is not CLI-runner-shaped, so a plain library dependency can
/// never be exempted by a coincidental script command head.
///
/// A package's `bin` field is the authoritative binary name but lives in
/// node_modules, which is not read here. CLI-runner packages follow naming
/// conventions: a scoped `@scope/cli` ships a binary named after the scope
/// (`@manypkg/cli` → `manypkg`) — sometimes the scope with a plural `s` dropped
/// (`@changesets/cli` → `changeset`); an unscoped `foo-cli` / `foo-bin` ships
/// `foo` (or the full `foo-cli`). Only these shapes yield candidates.
fn cli_runner_binary_candidates(name: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(scope) = name.strip_prefix('@') {
        if let Some((scope, sub)) = scope.split_once('/')
            && sub == "cli"
            && !scope.is_empty()
        {
            candidates.push(scope.to_string());
            if let Some(singular) = scope.strip_suffix('s') {
                candidates.push(singular.to_string());
            }
        }
        return candidates;
    }
    if let Some(base) = name.strip_suffix("-cli").or_else(|| name.strip_suffix("-bin")) {
        candidates.push(name.to_string());
        if !base.is_empty() {
            candidates.push(base.to_string());
        }
    }
    candidates
}

/// Normalize a `main` value (a relative file path) to the shape consumers
/// compare against: forward slashes, optional leading `./` stripped. `main`
/// values are bare relative (`index.js`, `dist/index.js`) or `./`-prefixed.
fn normalize_main_path(target: &str) -> Option<String> {
    let rel = target.strip_prefix("./").unwrap_or(target);
    if rel.is_empty() {
        return None;
    }
    Some(rel.replace('\\', "/"))
}

/// Normalize an `exports` target. Per the Node spec an `exports` file target
/// must start with `./`; a value without it is a bare specifier (a re-export of
/// another package, not a file here), so reject it.
fn normalize_export_path(target: &str) -> Option<String> {
    let rel = target.strip_prefix("./")?;
    if rel.is_empty() {
        return None;
    }
    Some(rel.replace('\\', "/"))
}

/// Recursively gather every relative target string out of an `exports`
/// conditions value. A condition value is a string (`"./index.js"`) or a nested
/// object keyed by condition (`{ "import": "./x.mjs", "require": "./x.cjs" }`).
fn collect_export_targets(node: &Value, out: &mut BTreeSet<String>) {
    match node {
        Value::String(s) => {
            if let Some(rel) = normalize_export_path(s) {
                out.insert(rel);
            }
        }
        Value::Object(map) => {
            for value in map.values() {
                collect_export_targets(value, out);
            }
        }
        _ => {}
    }
}

/// Collect the substitute targets of a `browser`/`react-native` field. A string
/// (`"browser": "./dist/browser.js"`) is the single substitute; an object is a
/// substitution map whose VALUES are the substitute files swapped in at bundle
/// time. The KEYS are normal imported node files (already reachable via the
/// import graph), so only string values are collected — non-string values are
/// webpack's `"./x": false` "ignore this module" form and name no file.
fn collect_substitute_targets(node: &Value, out: &mut BTreeSet<String>) {
    match node {
        Value::String(s) => {
            if let Some(rel) = normalize_main_path(s) {
                out.insert(rel);
            }
        }
        Value::Object(map) => {
            for value in map.values() {
                if let Some(s) = value.as_str()
                    && let Some(rel) = normalize_main_path(s)
                {
                    out.insert(rel);
                }
            }
        }
        _ => {}
    }
}

/// The relative paths this package declares as its own entry point: the `main`
/// value plus the `exports` `.` target(s) (including conditional `import`/
/// `require`/`default` variants). A string `exports` (no subpath map) is itself
/// the `.` target. Also includes the `browser` and `react-native` substitute
/// targets — the browser/native build of the library that bundlers swap in at
/// build time, reachable only through the substitution map, never `import`ed.
fn collect_entry_files(json: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Some(main) = json.get("main").and_then(Value::as_str)
        && let Some(rel) = normalize_main_path(main)
    {
        out.insert(rel);
    }
    match json.get("exports") {
        Some(Value::String(s)) => {
            if let Some(rel) = normalize_export_path(s) {
                out.insert(rel);
            }
        }
        Some(Value::Object(map)) => {
            if let Some(dot) = map.get(".") {
                collect_export_targets(dot, &mut out);
            }
        }
        _ => {}
    }
    if let Some(browser) = json.get("browser") {
        collect_substitute_targets(browser, &mut out);
    }
    if let Some(native) = json.get("react-native") {
        collect_substitute_targets(native, &mut out);
    }
    out
}

/// True when every published entry path of `json` lives outside a top-level
/// `src/` directory, and at least one such entry exists. This is the signal that
/// `src/` is build *input* compiled away into the published artifact (e.g.
/// monaco-editor whose `main` is `./min/...` and `module` is `./esm/...`): the
/// shipped bundle inlines its build-time dependencies, so `src/` files importing
/// a devDependency carry no runtime dependency. Considers `main`, `module`, every
/// `exports` target (every subpath, not just `.`), and the `browser`/
/// `react-native` substitutes. Returns false when a published entry IS under
/// `src/` — that package ships its source, so `src/` is runtime code.
fn entries_outside_src(json: &Value) -> bool {
    let mut targets = BTreeSet::new();
    if let Some(main) = json.get("main").and_then(Value::as_str)
        && let Some(rel) = normalize_main_path(main)
    {
        targets.insert(rel);
    }
    if let Some(module) = json.get("module").and_then(Value::as_str)
        && let Some(rel) = normalize_main_path(module)
    {
        targets.insert(rel);
    }
    if let Some(exports) = json.get("exports") {
        collect_export_targets(exports, &mut targets);
    }
    if let Some(browser) = json.get("browser") {
        collect_substitute_targets(browser, &mut targets);
    }
    if let Some(native) = json.get("react-native") {
        collect_substitute_targets(native, &mut targets);
    }
    !targets.is_empty() && targets.iter().all(|rel| !rel.starts_with("src/"))
}

/// File stem (basename without its final extension) of a relative target path,
/// e.g. `dist/es/index.mjs` → `index`, `dist/cjs/dom.js` → `dom`. Compound
/// extensions like `.d.ts` collapse to the base name (`dom.d.ts` → `dom`).
fn entry_target_stem(rel: &str) -> Option<String> {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    let stem = file.split('.').next().unwrap_or(file);
    if stem.is_empty() {
        return None;
    }
    Some(stem.to_string())
}

/// Stems of every published entry of `json` — every `exports` subpath target
/// plus `main` and `module`. The stems identify the package's distinct public
/// entry points independent of the built file's directory or extension, so a
/// source barrel can be matched to the entry it compiles into.
fn collect_export_entry_stems(json: &Value) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    if let Some(main) = json.get("main").and_then(Value::as_str)
        && let Some(rel) = normalize_main_path(main)
    {
        targets.insert(rel);
    }
    if let Some(module) = json.get("module").and_then(Value::as_str)
        && let Some(rel) = normalize_main_path(module)
    {
        targets.insert(rel);
    }
    if let Some(exports) = json.get("exports") {
        collect_export_targets(exports, &mut targets);
    }
    targets.iter().filter_map(|rel| entry_target_stem(rel)).collect()
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

/// Workspace package globs from the `workspaces` field, supporting both
/// declaration shapes: the npm/Yarn-classic array (`"workspaces": ["packages/*"]`)
/// and the Yarn Berry / pnpm nested object (`"workspaces": {"packages": [...]}`),
/// whose `packages` array carries the same globs. Any other shape yields no globs.
fn parse_workspaces(json: &Value) -> Vec<String> {
    let node = match json.get("workspaces") {
        Some(node) => node,
        None => return Vec::new(),
    };
    let globs = match node {
        Value::Array(arr) => arr,
        Value::Object(obj) => match obj.get("packages").and_then(Value::as_array) {
            Some(arr) => arr,
            None => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    globs
        .iter()
        .filter_map(|node| node.as_str().map(String::from))
        .collect()
}

/// Smallest major version a semver range string can match, or `None` when the
/// range names no major version. Each version token contributes the integer
/// before its first `.` (range operators `^ ~ >= <= > < =` and whitespace are
/// ignored); the minimum across all tokens is returned. There is no semver
/// crate in this workspace, so this stays a lexical heuristic over the tokens.
/// `>=18.0.0` → 18, `^18 || ^19` → 18, `18.x` → 18, `>=19.0.0` → 19.
fn min_major_version(range: &str) -> Option<u32> {
    range
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '.')
        .filter_map(|token| {
            let major = token.trim_start_matches(|c: char| !c.is_ascii_digit());
            let major = major.split('.').next()?;
            major.parse::<u32>().ok()
        })
        .min()
}

#[derive(Debug, Clone, Default)]
pub struct Tsconfig {
    pub paths: BTreeMap<String, Vec<String>>,
    pub base_url: Option<PathBuf>,
    pub module: Option<String>,
    pub module_resolution: Option<String>,
    pub strict: bool,
    pub exact_optional_property_types: bool,
    pub jsx: Option<String>,
    pub out_dir: Option<PathBuf>,
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
            exact_optional_property_types: co
                .and_then(|x| x.get("exactOptionalPropertyTypes"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false),
            jsx: co
                .and_then(|x| x.get("jsx"))
                .and_then(|s| s.as_str())
                .map(String::from),
            out_dir: co
                .and_then(|x| x.get("outDir"))
                .and_then(|s| s.as_str())
                .map(PathBuf::from),
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
        exact_optional_property_types: co
            .and_then(|x| x.get("exactOptionalPropertyTypes"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false),
        jsx: co
            .and_then(|x| x.get("jsx"))
            .and_then(|s| s.as_str())
            .map(String::from),
        out_dir: co
            .and_then(|x| x.get("outDir"))
            .and_then(|s| s.as_str())
            .map(PathBuf::from),
    }
}

/// Overlay `child` onto `parent`. Scalars (`base_url`, `module`,
/// `module_resolution`, `jsx`, `out_dir`) are taken from the child when present;
/// `paths`
/// are merged key-by-key so parent-only aliases survive. Boolean flags
/// (`strict`, `exact_optional_property_types`) default to false in
/// `parse_tsconfig_value`, so a child that omits the flag inherits the parent's
/// value here via the `||`.
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
        exact_optional_property_types: child.exact_optional_property_types
            || parent.exact_optional_property_types,
        jsx: child.jsx.or(parent.jsx),
        out_dir: child.out_dir.or(parent.out_dir),
    }
}

/// Parsed `Cargo.toml` manifest, classified for the Rust lint rules. Built
/// once per manifest directory by [`ProjectCtx::nearest_cargo_manifest`] and
/// shared via `Arc`. Stores the manifest *directory* so `is_binary_only` can
/// stat `src/lib.rs` next to it.
#[derive(Debug, Clone)]
pub struct CargoManifest {
    /// Directory containing the `Cargo.toml`.
    manifest_dir: PathBuf,
    /// `[lib]` table is present.
    has_lib_table: bool,
    /// An async runtime (`tokio`, `async-std`, `futures`) is declared in any
    /// dependency section.
    async_runtime: bool,
    /// `[package].categories` lists `"no-std"`.
    no_std_category: bool,
}

impl CargoManifest {
    /// Async runtimes whose presence in any dependency section marks the crate
    /// as async.
    const ASYNC_RUNTIMES: &'static [&'static str] =
        &["tokio", "async-std", "async_std", "futures"];

    /// Parse a `Cargo.toml`'s raw text. `manifest_dir` is the directory holding
    /// the manifest (kept for the `src/lib.rs` filesystem check). Returns `None`
    /// when the text is not valid TOML.
    pub fn parse(raw: &str, manifest_dir: PathBuf) -> Option<Self> {
        let value = raw.parse::<toml::Value>().ok()?;

        let has_lib_table = value.get("lib").is_some();

        let async_runtime = ["dependencies", "dev-dependencies", "build-dependencies"]
            .iter()
            .filter_map(|section| value.get(section).and_then(toml::Value::as_table))
            .any(|table| Self::ASYNC_RUNTIMES.iter().any(|rt| table.contains_key(*rt)));

        let no_std_category = value
            .get("package")
            .and_then(|package| package.get("categories"))
            .and_then(toml::Value::as_array)
            .is_some_and(|categories| {
                categories
                    .iter()
                    .any(|category| category.as_str() == Some("no-std"))
            });

        Some(CargoManifest {
            manifest_dir,
            has_lib_table,
            async_runtime,
            no_std_category,
        })
    }

    /// True when the crate builds no library target: no `[lib]` table and no
    /// `src/lib.rs` next to the manifest.
    pub fn is_binary_only(&self) -> bool {
        !self.has_lib_table && !self.manifest_dir.join("src/lib.rs").is_file()
    }

    /// True when the crate depends on an async runtime.
    pub fn has_async_runtime(&self) -> bool {
        self.async_runtime
    }

    /// True when `[package].categories` lists `"no-std"`.
    pub fn is_no_std(&self) -> bool {
        self.no_std_category
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
    /// User-configured entrypoints globs (from `comply.toml`). Empty by default.
    pub entrypoint_globs: Vec<String>,

    // Per-manifest caches, keyed by the *directory* that contains the
    // manifest. Mutex over HashMap is sufficient: contention is low (same
    // manifest reused across sibling files hits the cache, so after the
    // first insert all readers take the lock briefly just to clone an Arc).
    package_json_cache: Mutex<HashMap<PathBuf, Arc<PackageJson>>>,
    tsconfig_cache: Mutex<HashMap<PathBuf, Arc<Tsconfig>>>,
    cargo_manifest_cache: Mutex<HashMap<PathBuf, Arc<CargoManifest>>>,

    // Memoizes the upward `walk_up_finding` stat-walk that locates a marker
    // file (`package.json`, `tsconfig.json`). The resolved manifest directory
    // is identical for every file in the same directory, so the walk runs once
    // per (start dir, marker) instead of once per file. Nested by marker so
    // hits avoid allocating a composite key.
    manifest_dir_cache: Mutex<HashMap<&'static str, HashMap<PathBuf, Option<PathBuf>>>>,

    // Lazy project-wide fields. `OnceLock<Option<T>>` keeps the "init once,
    // cache None on failure, never retry" contract in a single primitive.
    tailwind_theme: OnceLock<Option<TailwindTheme>>,
    drizzle_config: OnceLock<Option<DrizzleConfig>>,

    // "Does this project use Tailwind?" Probed once: a `tailwind.config.*`
    // file at the project/workspace root or a `tailwindcss` / `@tailwindcss/*`
    // dependency. Tailwind-utility rules use it to skip projects that style
    // with CSS-in-JS, where classes like `focus:ring-*` are meaningless.
    uses_tailwind: OnceLock<bool>,

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

    // Memoized once-per-project anchor. `anchor_path()` is invariant after
    // load (the index and `linted_paths` are frozen), but ~5 cross-file rules
    // call it on every file — caching collapses an O(N) `min()` scan per
    // (rule × file) into one computation.
    anchor_path_cache: OnceLock<Option<PathBuf>>,

    // "Does this project use React Compiler?" keyed by the *directory* of the
    // file asking. The answer depends only on the directory chain (manifest +
    // bundler/babel configs from that dir up to the root), not file content,
    // and the underlying probe stat-walks config files — so without this memo a
    // JSX-dense tree pays the full walk once per file.
    react_compiler_dir_cache: Mutex<HashMap<PathBuf, bool>>,

    // "Does this project use a bundler?" keyed by the *directory* of the file
    // asking. Like `react_compiler_dir_cache`, the answer depends only on the
    // directory chain (nearest package.json + bundler config files up to the
    // root), not file content, and the probe stat-walks config files — so
    // without this memo a deep monorepo pays the full walk once per file.
    bundler_dir_cache: Mutex<HashMap<PathBuf, bool>>,

    // Workspace member package names, read+parsed from each workspace root's
    // package.json. Project-wide and constant, but queried once per import by
    // `no-implicit-deps` / `unlisted-dependency` — memoized so the disk read +
    // JSON parse of every member manifest happens once, not once per import.
    workspace_package_names_cache: OnceLock<Vec<String>>,

    // Union of every dependency name declared in every `package.json` under the
    // project root tree (excluding `node_modules`), keyed by the resolved root
    // directory. Monorepos that don't declare a `workspaces` field (so the
    // workspace walk never runs) still hoist sibling packages' deps at runtime;
    // this lets `no-implicit-deps` recognize a dep declared in any sibling
    // manifest. Built lazily on first miss and reused for the rest of the run.
    tree_dep_names_cache: Mutex<HashMap<PathBuf, Arc<HashSet<String>>>>,

    // Union of every dependency name declared across all member packages of an
    // npm-workspaces root, keyed by that root's directory. npm hoists every
    // member's deps to the shared root `node_modules`, so any member may import a
    // specifier declared only in a sibling member; this lets `no-implicit-deps`
    // recognize such an import. Resolved from the `workspaces` globs (not a full
    // tree walk), so it covers the workspaces root even when `project_root` is
    // scoped to one member. Built lazily on first miss and reused for the run.
    workspace_sibling_deps_cache: Mutex<HashMap<PathBuf, Arc<HashSet<String>>>>,

    // Files the engine read and found to contain no `comply-ignore` substring.
    // The post-filter (`ignore_comments::apply_to_all`) otherwise re-reads every
    // discovered file from disk just to run that one substring check; for files
    // recorded here it can skip the read entirely (a known-clean file can carry
    // neither a suppression nor a malformed marker). Keyed by the discovery path
    // (same value `apply_to_all` iterates), so no canonicalization is needed.
    clean_files: Mutex<HashSet<PathBuf>>,

    // Prisma model names (lowercase) that have a `deletedAt` field in the
    // project's schema.prisma. `None` when no schema.prisma is found (rules
    // fall back to the old "fire on all" behaviour). Populated lazily on
    // first access, cached for the lifetime of the run.
    prisma_soft_delete_models: OnceLock<Option<HashSet<String>>>,

    // Frameworks detected from the *nearest* package.json to a file, keyed by
    // that manifest's directory. Root-level `detected_frameworks` misses an app
    // nested in a subdirectory (a Next.js example under a library's `app/`, or
    // any monorepo package) because detection only reads the root manifest; this
    // resolves the framework owning each file. Memoized per manifest dir — the
    // answer is identical for every file under the same package.json.
    path_frameworks_cache: Mutex<HashMap<PathBuf, Vec<&'static FrameworkDef>>>,

    // `lib.entryFile` declared in each `ng-package.json`, keyed by that file's
    // directory. ng-packagr Angular libraries declare their public-API entry
    // here, not in `package.json` `main`/`exports` (those are emitted to the
    // build output). Parsed lazily on first miss and memoized — the answer is
    // identical for every file under the same `ng-package.json`. `None` caches a
    // missing/malformed file or an absent `lib.entryFile` so it is not re-read.
    ng_package_entry_cache: Mutex<HashMap<PathBuf, Option<String>>>,
}

impl ProjectCtx {
    /// Empty instance — used by `default_static_project_ctx` and by the LSP
    /// path when no workspace context is available. `nearest_*` accessors
    /// still walk disk; only the eager root-level fields are absent.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Empty instance with `uses_tailwind()` forced to `true`. Lets Tailwind
    /// rule unit tests exercise their class-matching logic without staging a
    /// `tailwind.config` on disk.
    #[cfg(test)]
    pub fn empty_with_tailwind() -> Self {
        let ctx = Self::default();
        ctx.uses_tailwind.set(true).unwrap();
        ctx
    }

    /// Record that the engine read `path` and found no `comply-ignore`
    /// substring. Called once per file from the parallel engine loop, so the
    /// lock is held only for the insert.
    pub fn note_clean_file(&self, path: &Path) {
        self.clean_files.lock().unwrap().insert(path.to_path_buf());
    }

    /// Snapshot of the known-clean file set, taken once after the engine
    /// completes so the post-filter can do lock-free membership checks.
    #[must_use]
    pub fn clean_files_snapshot(&self) -> HashSet<PathBuf> {
        self.clean_files.lock().unwrap().clone()
    }

    /// Load once per run from the set of files being linted. Eagerly parses
    /// every TS/JS/TSX input to build `import_index` — cross-file rules are
    /// noisy/wrong without it, so we don't make that lookup lazy.
    pub fn load(files: &[&SourceFile], config: &Config) -> Self {
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
            entrypoint_globs: config.entrypoints().to_vec(),
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
        self.anchor_path_cache
            .get_or_init(|| {
                if let Some(linted) = self.linted_paths.get() {
                    linted.iter().min().cloned()
                } else {
                    self.import_index().min_indexed_path().map(Path::to_path_buf)
                }
            })
            .clone()
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

    /// True when the project exposes HTTP API server boundaries — i.e. a
    /// dedicated HTTP server framework (Express, Hono, Elysia, NestJS) or a
    /// full-stack framework with server route handlers (Next.js, Remix, Nuxt,
    /// SvelteKit) is detected. Used by boundary-validation rules whose "parse
    /// once at the HTTP boundary, trust internally" principle only holds for
    /// API servers; CLI tools and pure libraries have no such boundary.
    pub fn is_http_api_server(&self) -> bool {
        const HTTP_SERVER_FRAMEWORKS: &[&str] = &[
            "express", "hono", "elysia", "nestjs", "nextjs", "remix", "nuxt", "svelte",
        ];
        self.detected_frameworks
            .iter()
            .any(|f| HTTP_SERVER_FRAMEWORKS.contains(&f.name.as_str()))
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

    /// True when the project uses Tailwind CSS — either a `tailwind.config.*`
    /// file (`.ts`, `.js`, `.cjs`, `.mjs`) sits at the project root or any
    /// workspace root, or `tailwindcss` / a `@tailwindcss/*` package is
    /// declared in the root manifest's dependencies. Used by Tailwind-utility
    /// rules to skip projects that style with CSS-in-JS (MUI, ant-design),
    /// where classes like `focus:ring-*` are meaningless. Cached for the run.
    pub fn uses_tailwind(&self) -> bool {
        *self.uses_tailwind.get_or_init(|| {
            const CONFIG_NAMES: &[&str] = &[
                "tailwind.config.ts",
                "tailwind.config.js",
                "tailwind.config.cjs",
                "tailwind.config.mjs",
            ];
            let has_config = self
                .project_root
                .iter()
                .chain(self.workspace_roots.iter())
                .any(|root| CONFIG_NAMES.iter().any(|name| root.join(name).metadata().is_ok()));
            if has_config {
                return true;
            }
            self.package_json.as_ref().is_some_and(|pkg| {
                pkg.all_deps()
                    .any(|dep| dep == "tailwindcss" || dep.starts_with("@tailwindcss/"))
            })
        })
    }

    /// True if `path` matches any user-configured entrypoints glob.
    /// Relativizes `path` against `project_root` (or CWD as fallback) before
    /// glob matching — same anchor as the rest of the import-graph logic.
    pub fn entrypoints_contains(&self, path: &Path) -> bool {
        use globset::Glob;
        if self.entrypoint_globs.is_empty() {
            return false;
        }
        let anchor = self
            .project_root
            .as_deref()
            .and_then(|r| std::fs::canonicalize(r).ok())
            .or_else(|| std::env::current_dir().ok());
        let rel: std::borrow::Cow<Path> = if path.is_absolute() {
            anchor
                .as_deref()
                .and_then(|a| path.strip_prefix(a).ok().map(|p| p.to_path_buf()))
                .map(std::borrow::Cow::Owned)
                .unwrap_or_else(|| std::borrow::Cow::Borrowed(path))
        } else {
            std::borrow::Cow::Borrowed(path.strip_prefix("./").unwrap_or(path))
        };
        for pattern in &self.entrypoint_globs {
            if let Ok(glob) = Glob::new(pattern) {
                if glob.compile_matcher().is_match(rel.as_ref()) {
                    return true;
                }
            }
        }
        false
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

    /// Frameworks owning `path`, detected from its nearest package.json.
    ///
    /// Root-level `detected_frameworks` only inspects the root manifest, so a
    /// framework app nested in a subdirectory (e.g. a Next.js example under a
    /// library's `app/`, or a monorepo package) goes undetected. This walks up
    /// to the package.json closest to `path` and detects frameworks from it,
    /// memoized by manifest directory.
    pub fn frameworks_for_path(&self, path: &Path) -> Vec<&'static FrameworkDef> {
        let Some(manifest_dir) = self.nearest_package_json_dir(path) else {
            return Vec::new();
        };
        if let Some(found) = self.path_frameworks_cache.lock().unwrap().get(&manifest_dir) {
            return found.clone();
        }
        let detected = self
            .nearest_package_json(path)
            .map(|pkg| crate::frameworks::detect_frameworks(&pkg, Some(&manifest_dir)))
            .unwrap_or_default();
        self.path_frameworks_cache
            .lock()
            .unwrap()
            .insert(manifest_dir, detected.clone());
        detected
    }

    /// Magic export names recognized for `path`: the union of the root-detected
    /// frameworks' magic exports and those of the framework owning `path` via its
    /// nearest `package.json`. A magic export (Next.js `metadata`/`default`,
    /// `generateStaticParams`, …) is consumed by the framework's file-system
    /// router by convention, never through a static import. Walking the nearest
    /// manifest lets these be recognized for an app nested in a sub-package whose
    /// framework dependency is invisible to root-anchored detection.
    pub fn magic_exports_for_path(&self, path: &Path) -> HashSet<&str> {
        let mut names: HashSet<&str> = self.framework_magic_exports().collect();
        for fw in self.frameworks_for_path(path) {
            names.extend(fw.magic_exports.names.iter().map(String::as_str));
        }
        self.extend_route_magic_exports(path, &mut names);
        names
    }

    /// Add a framework's route-scoped magic exports when `path` matches the file
    /// convention that consumes them. SvelteKit reserves `load`/`ssr`/`csr`/… in
    /// `+page`/`+layout`/`+server` route files and `match` in `src/params/*`; the
    /// router calls each by exact name, so they have no importer but are live.
    /// Scoping by file convention keeps a same-named export in an ordinary module
    /// flaggable.
    fn extend_route_magic_exports<'a>(&'a self, path: &Path, names: &mut HashSet<&'a str>) {
        let basename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_route_file = crate::rules::path_utils::is_sveltekit_route_file(basename);
        let is_param_matcher = crate::rules::path_utils::is_sveltekit_param_matcher_file(path);
        if !is_route_file && !is_param_matcher {
            return;
        }
        // Only frameworks detected for this path (root manifest + nearest
        // package.json) contribute, so a non-SvelteKit `+page.ts` stays
        // unaffected. SvelteKit is the only framework declaring these today.
        let owning = self
            .detected_frameworks
            .iter()
            .copied()
            .chain(self.frameworks_for_path(path));
        for fw in owning {
            if is_route_file {
                names.extend(fw.route_magic_exports.route_files.iter().map(String::as_str));
            }
            if is_param_matcher {
                names.extend(fw.route_magic_exports.param_matchers.iter().map(String::as_str));
            }
        }
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

    /// Walk up from `path` to the nearest `package.json` and return the
    /// *directory* containing it. The walk result is cached by start directory —
    /// the same invariant as `nearest_package_json`.
    pub fn nearest_package_json_dir(&self, path: &Path) -> Option<PathBuf> {
        let start_dir = path.parent()?;
        walk_up_finding_cached(&self.manifest_dir_cache, start_dir, "package.json")
    }

    /// Walk up from `path` to the nearest Deno config (`deno.json` or
    /// `deno.jsonc`) and return the *directory* containing it. `deno.json` is
    /// Deno's authoritative manifest, declaring its own import map; a file under
    /// such a directory belongs to a Deno subtree, not the surrounding npm
    /// project. When both names sit at different depths the closer (deeper) one
    /// wins. Shares the manifest-dir cache with the other `nearest_*` walks.
    pub fn nearest_deno_config_dir(&self, path: &Path) -> Option<PathBuf> {
        let start_dir = path.parent()?;
        let json = walk_up_finding_cached(&self.manifest_dir_cache, start_dir, "deno.json");
        let jsonc = walk_up_finding_cached(&self.manifest_dir_cache, start_dir, "deno.jsonc");
        match (json, jsonc) {
            (Some(a), Some(b)) => Some(deeper_dir(a, b)),
            (found, None) | (None, found) => found,
        }
    }

    /// Walk up from `path` to the nearest `package.json`, cache the parsed
    /// result by manifest directory. Returns the same `Arc` on repeated
    /// lookups against any file under the same manifest.
    pub fn nearest_package_json(&self, path: &Path) -> Option<Arc<PackageJson>> {
        nearest(
            &self.package_json_cache,
            &self.manifest_dir_cache,
            path,
            "package.json",
            PackageJson::parse,
        )
    }

    /// True when `path` is the entry point its own `package.json` declares —
    /// the file named by `main` or the `exports` `.` target of the nearest
    /// manifest. Such a file's job is to dispatch to the built artifact (e.g. a
    /// CJS root that `require`s `./dist/...` based on `NODE_ENV`); rules about
    /// "import from the package entry point" must not fire on the entry itself.
    pub fn is_package_entry_file(&self, path: &Path) -> bool {
        let Some(manifest_dir) = self.nearest_package_json_dir(path) else {
            return false;
        };
        let Some(pkg) = self.nearest_package_json(path) else {
            return false;
        };
        pkg.entry_files
            .iter()
            .any(|entry| manifest_dir.join(entry) == path)
    }

    /// True when `path` is invoked directly as a CLI entry by its nearest
    /// `package.json` `scripts` (e.g. `"build": "tsx ./build.ts"` makes the
    /// sibling `build.ts` a script entry). Such a file is run as a one-shot
    /// executable by a runner, never `import`-ed by another module and never
    /// part of the published `dist/`, so rules that constrain *published module*
    /// semantics (e.g. `node-no-top-level-await`) must not fire on it. The
    /// extracted entries are manifest-dir-relative, so the comparison joins each
    /// onto the manifest directory and matches against `path`.
    pub fn is_script_entry_file(&self, path: &Path) -> bool {
        let Some(manifest_dir) = self.nearest_package_json_dir(path) else {
            return false;
        };
        let Some(pkg) = self.nearest_package_json(path) else {
            return false;
        };
        pkg.script_entry_files
            .iter()
            .any(|entry| manifest_dir.join(entry) == path)
    }

    /// True when `path` lives in a subdirectory that houses a `package.json`
    /// `scripts` entry file (e.g. `omnidoc/generateApiDoc.ts` is run by
    /// `"omnidoc": "tsx ./omnidoc/generateApiDoc.ts"`, marking the whole
    /// `omnidoc/` directory as build tooling). Such a directory is a one-shot
    /// codegen/doc-generation toolchain run at build time, never published, so
    /// its sibling helper modules — which the entry imports but no script names
    /// directly — are dev tooling too. Generalizes the script-entry signal from
    /// the named file to its directory.
    ///
    /// Scoped to subdirectories of the manifest: a script entry sitting directly
    /// at the manifest root (e.g. `build.ts`) does not mark the root — where
    /// published source also lives — as a tooling directory.
    pub fn is_in_script_entry_dir(&self, path: &Path) -> bool {
        let Some(manifest_dir) = self.nearest_package_json_dir(path) else {
            return false;
        };
        let Some(pkg) = self.nearest_package_json(path) else {
            return false;
        };
        let Some(parent) = path.parent() else {
            return false;
        };
        pkg.script_entry_files.iter().any(|entry| {
            let entry_path = manifest_dir.join(entry);
            entry_path.parent() == Some(parent) && parent != manifest_dir
        })
    }

    /// True when `path`'s file stem matches one of the published entry-point
    /// stems its nearest `package.json` declares (any `exports` subpath, plus
    /// `main`/`module`). A multi-entry package ships built artifacts under
    /// `dist/` whose stems (`index`, `dom`, ...) carry over to the source
    /// barrels (`src/index.ts`, `src/dom.ts`); matching by stem identifies those
    /// source files as distinct public entry points even though the declared
    /// targets point at the build output, not the source.
    pub fn is_declared_entry_barrel(&self, path: &Path) -> bool {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            return false;
        };
        let Some(pkg) = self.nearest_package_json(path) else {
            return false;
        };
        pkg.export_entry_stems.contains(stem)
    }

    /// True when `path` is the public-API entry file an ng-packagr Angular
    /// library declares — the `lib.entryFile` of the nearest `ng-package.json`.
    /// ng-packagr libraries publish their entry through the build output's
    /// `package.json` (`main`/`exports`), not the source `package.json`, so the
    /// source entry and everything it re-exports look unimported. Rules about
    /// "this symbol has no importer" (e.g. `dead-export`) treat this file as a
    /// package entry point. `path` must be absolute; the comparison joins the
    /// manifest-relative `entryFile` onto the `ng-package.json`'s directory.
    pub fn is_ng_package_entry_file(&self, path: &Path) -> bool {
        let Some(manifest_dir) = self.nearest_ng_package_dir(path) else {
            return false;
        };
        let Some(entry_file) = self.ng_package_entry_file(&manifest_dir) else {
            return false;
        };
        manifest_dir.join(entry_file) == path
    }

    /// Walk up from `path` to the directory of the nearest `ng-package.json`.
    /// Shares the manifest-dir cache with the other `nearest_*` walks.
    fn nearest_ng_package_dir(&self, path: &Path) -> Option<PathBuf> {
        let start_dir = path.parent()?;
        walk_up_finding_cached(&self.manifest_dir_cache, start_dir, "ng-package.json")
    }

    /// Directory of the smallest published package surface enclosing `path`.
    ///
    /// An ng-packagr library publishes secondary entry points (`@scope/lib`,
    /// `@scope/lib/common`, `@scope/lib/standalone`) as nested `ng-package.json`
    /// directories that share the library's single `package.json`. Each is an
    /// independent public package even though they live under one manifest, so
    /// the nearest `ng-package.json` directory (deepest, most specific) is the
    /// true boundary. Falls back to the nearest `package.json` directory when no
    /// `ng-package.json` lies between `path` and that manifest.
    pub fn package_boundary_dir(&self, path: &Path) -> Option<PathBuf> {
        self.nearest_ng_package_dir(path)
            .or_else(|| self.nearest_package_json_dir(path))
    }

    /// `lib.entryFile` of the `ng-package.json` in `manifest_dir`, parsed once
    /// and memoized by directory. `None` for a missing/malformed file or one
    /// without a `lib.entryFile` string.
    fn ng_package_entry_file(&self, manifest_dir: &Path) -> Option<String> {
        if let Some(hit) = self.ng_package_entry_cache.lock().ok()?.get(manifest_dir) {
            return hit.clone();
        }
        let raw = std::fs::read_to_string(manifest_dir.join("ng-package.json")).ok();
        let entry = raw.as_deref().and_then(parse_ng_package_entry_file);
        if let Ok(mut map) = self.ng_package_entry_cache.lock() {
            map.entry(manifest_dir.to_path_buf())
                .or_insert_with(|| entry.clone());
        }
        entry
    }

    /// True when `path` is a build-input source file: it sits under the nearest
    /// manifest's top-level `src/` directory, and that package publishes its
    /// entries (`main`/`module`/`exports`/…) from outside `src/`. The published
    /// artifact is compiled output elsewhere (`dist/`, `esm/`, `min/`, …) that
    /// inlines build-time dependencies, so the `src/` tree is never shipped as-is.
    /// Rules treating `src/` as runtime production code (e.g. the devDependency
    /// check in `no-extraneous-import`) must not fire here: a devDependency
    /// imported from build input is bundled at build time, not a runtime import.
    pub fn is_bundled_build_input(&self, path: &Path) -> bool {
        let Some(manifest_dir) = self.nearest_package_json_dir(path) else {
            return false;
        };
        let Some(pkg) = self.nearest_package_json(path) else {
            return false;
        };
        pkg.entries_outside_src && path.starts_with(manifest_dir.join("src"))
    }

    /// True when the React version range the project depends on still admits
    /// React 18 (so `forwardRef` remains required — React 18 has no ref-as-prop
    /// API). Reads the `react` range from peerDependencies, then dependencies,
    /// then devDependencies of the nearest package.json. Returns false when no
    /// React range is declared (rule keeps firing) or when the range requires
    /// React 19+.
    pub fn react_supports_v18(&self, path: &Path) -> bool {
        let Some(pkg) = self.nearest_package_json(path) else {
            return false;
        };
        let range = pkg
            .peer_dependencies
            .get("react")
            .or_else(|| pkg.dependencies.get("react"))
            .or_else(|| pkg.dev_dependencies.get("react"));
        range
            .and_then(|r| min_major_version(r))
            .is_some_and(|m| m <= 18)
    }

    /// True when Vitest is the test runner governing `path`. Vitest with
    /// `@testing-library/react` auto-runs `cleanup()` after each test, so a
    /// manual `afterEach(cleanup)` is redundant only here; under Jest it is the
    /// documented, required pattern. Evidence, in order: `vitest` declared in any
    /// dep section of the nearest package.json, a `scripts` entry that invokes
    /// `vitest`, or a `vitest.config.*` file in the directory walk up to the
    /// project root. Returns false when none is present, so test-runner-specific
    /// rules stay silent in Jest (and ambiguous) projects.
    pub fn uses_vitest(&self, path: &Path) -> bool {
        if let Some(pkg) = self.nearest_package_json(path)
            && (pkg.has_dep_or_engine("vitest") || pkg.scripts_invoke_test_runner("vitest"))
        {
            return true;
        }
        self.has_vitest_config(path)
    }

    /// True when a `vitest.config.*` (or `vite.config.*` declaring a `test`
    /// block) sits between `path`'s directory and the project root. Only the
    /// dedicated `vitest.config.*` name is treated as a signal — a plain
    /// `vite.config.*` may belong to a build that does not run Vitest, so its
    /// presence alone is not evidence of the runner.
    fn has_vitest_config(&self, path: &Path) -> bool {
        const VITEST_CONFIG_FILES: &[&str] = &[
            "vitest.config.ts",
            "vitest.config.js",
            "vitest.config.mts",
            "vitest.config.mjs",
            "vitest.config.cts",
            "vitest.config.cjs",
            "vitest.workspace.ts",
            "vitest.workspace.js",
        ];

        // Upper bound for the config-file walk: the explicit project root, else
        // the first ancestor that owns a `package.json`. Never escapes upward.
        let stop_at: Option<PathBuf> = self.project_root.clone().or_else(|| {
            let mut d = path.parent();
            loop {
                let Some(dir) = d else { break None };
                if dir.join("package.json").is_file() {
                    break Some(dir.to_path_buf());
                }
                d = dir.parent();
            }
        });

        let mut dir = path.parent();
        while let Some(d) = dir {
            if VITEST_CONFIG_FILES.iter().any(|name| d.join(name).is_file()) {
                return true;
            }
            if stop_at.as_deref() == Some(d) {
                break;
            }
            dir = d.parent();
        }
        false
    }

    /// True when the project ships React Compiler — declared as a dependency
    /// or referenced from a bundler / babel config between `path`'s directory
    /// and the project root. Memoized by directory: the answer is identical for
    /// every file in the same directory, so a JSX-dense tree pays the
    /// config-file stat-walk once per directory instead of once per file.
    pub fn uses_react_compiler(&self, path: &Path) -> bool {
        let key = path.parent().map(Path::to_path_buf).unwrap_or_default();
        if let Some(&v) = self.react_compiler_dir_cache.lock().unwrap().get(&key) {
            return v;
        }
        let v = self.compute_uses_react_compiler(path);
        self.react_compiler_dir_cache
            .lock()
            .unwrap()
            .insert(key, v);
        v
    }

    /// Memoize a directory-invariant "does this project use a bundler?" probe.
    /// The answer is identical for every file in the same directory (it depends
    /// only on the manifest + bundler-config chain from that directory up to the
    /// root), so a deep monorepo pays the stat-walk once per directory instead of
    /// once per file. `compute` runs at most once per directory.
    pub fn cached_bundler<F: FnOnce() -> bool>(&self, path: &Path, compute: F) -> bool {
        let key = path.parent().map(Path::to_path_buf).unwrap_or_default();
        if let Some(&v) = self.bundler_dir_cache.lock().unwrap().get(&key) {
            return v;
        }
        let v = compute();
        self.bundler_dir_cache.lock().unwrap().insert(key, v);
        v
    }

    fn compute_uses_react_compiler(&self, path: &Path) -> bool {
        const REACT_COMPILER_DEP: &str = "babel-plugin-react-compiler";
        const COMPILER_CONFIG_FILES: &[&str] = &[
            "vite.config.ts",
            "vite.config.js",
            "vite.config.mts",
            "vite.config.mjs",
            "vite.config.cts",
            "vite.config.cjs",
            "next.config.ts",
            "next.config.js",
            "next.config.mjs",
            "next.config.cjs",
            "babel.config.ts",
            "babel.config.js",
            "babel.config.mjs",
            "babel.config.cjs",
            "babel.config.json",
            ".babelrc",
            ".babelrc.json",
            ".babelrc.js",
            ".babelrc.cjs",
        ];

        if let Some(pkg) = self.nearest_package_json(path)
            && pkg.has_dep_or_engine(REACT_COMPILER_DEP)
        {
            return true;
        }

        // Upper bound for the config-file walk: the explicit project root, else
        // the first ancestor that owns a `package.json`. Never escapes upward.
        let stop_at: Option<PathBuf> = self.project_root.clone().or_else(|| {
            let mut d = path.parent();
            loop {
                let Some(dir) = d else { break None };
                if dir.join("package.json").is_file() {
                    break Some(dir.to_path_buf());
                }
                d = dir.parent();
            }
        });

        let mut dir = path.parent();
        while let Some(d) = dir {
            for name in COMPILER_CONFIG_FILES {
                let cfg = d.join(name);
                if !cfg.is_file() {
                    continue;
                }
                if let Ok(raw) = std::fs::read_to_string(&cfg)
                    && raw.contains(REACT_COMPILER_DEP)
                {
                    return true;
                }
            }
            if stop_at.as_deref() == Some(d) {
                break;
            }
            dir = d.parent();
        }
        false
    }

    /// Walk up from `path` to the nearest `tsconfig.json`, cache by manifest
    /// directory. Follows the `extends` chain so that settings inherited from
    /// a root `tsconfig.base.json` are visible to callers.
    pub fn nearest_tsconfig(&self, path: &Path) -> Option<Arc<Tsconfig>> {
        let start_dir = path.parent()?;
        let manifest_dir = walk_up_finding_cached(&self.manifest_dir_cache, start_dir, "tsconfig.json")?;

        if let Some(hit) = self.tsconfig_cache.lock().ok()?.get(&manifest_dir) {
            return Some(Arc::clone(hit));
        }

        let ts = load_tsconfig_file(&manifest_dir.join("tsconfig.json"), 0)?;
        let arc = Arc::new(ts);
        if let Ok(mut map) = self.tsconfig_cache.lock() {
            map.entry(manifest_dir).or_insert_with(|| Arc::clone(&arc));
        }
        Some(arc)
    }

    /// True when the tsconfig governing `path` enables
    /// `compilerOptions.exactOptionalPropertyTypes` (directly or inherited
    /// through its `extends` chain). Under that option `prop?: T` and
    /// `prop?: T | undefined` have distinct semantics — the latter additionally
    /// permits an explicit `undefined` assignment — so `| undefined` is *not*
    /// redundant with `?`. Defaults to false when no tsconfig is found.
    pub fn uses_exact_optional_property_types(&self, path: &Path) -> bool {
        self.nearest_tsconfig(path)
            .map(|tsc| tsc.exact_optional_property_types)
            .unwrap_or(false)
    }

    /// Walk up from `path` to the nearest `tsconfig.json` and return the
    /// *directory* containing it. Shares the manifest-dir cache and walk
    /// semantics with `nearest_tsconfig`.
    pub fn nearest_tsconfig_dir(&self, path: &Path) -> Option<PathBuf> {
        let start_dir = path.parent()?;
        walk_up_finding_cached(&self.manifest_dir_cache, start_dir, "tsconfig.json")
    }

    /// Absolute path of the compiled-output directory declared by the nearest
    /// tsconfig's `compilerOptions.outDir`, if any. Lets the import-resolution
    /// rules treat per-project build output (e.g. `lib/`) as a build artifact
    /// without hardcoding a directory name.
    pub fn tsconfig_out_dir(&self, path: &Path) -> Option<PathBuf> {
        let dir = self.nearest_tsconfig_dir(path)?;
        let tsc = self.nearest_tsconfig(path)?;
        tsc.out_dir.as_ref().map(|o| dir.join(o))
    }

    /// Walk up from `path` to the nearest `Cargo.toml`, returning the parsed
    /// manifest cached by manifest directory. The central accessor that the
    /// Rust lint rules query for crate shape (binary-only, async runtime,
    /// no-std) instead of each re-walking and re-parsing the manifest. Returns
    /// `None` when no `Cargo.toml` is found or it cannot be read or parsed —
    /// callers pick their own missing-manifest default.
    ///
    /// Cannot reuse the generic [`nearest`] helper because [`CargoManifest::parse`]
    /// needs the manifest directory (for the `src/lib.rs` stat), which that
    /// helper's `Fn(&str) -> Option<T>` parse signature cannot supply.
    pub fn nearest_cargo_manifest(&self, path: &Path) -> Option<Arc<CargoManifest>> {
        let start_dir = path.parent()?;
        let manifest_dir =
            walk_up_finding_cached(&self.manifest_dir_cache, start_dir, "Cargo.toml")?;

        if let Some(hit) = self.cargo_manifest_cache.lock().ok()?.get(&manifest_dir) {
            return Some(Arc::clone(hit));
        }

        let candidate = manifest_dir.join("Cargo.toml");
        let raw = std::fs::read_to_string(&candidate).ok()?;
        let manifest = match CargoManifest::parse(&raw, manifest_dir.clone()) {
            Some(manifest) => manifest,
            None => {
                eprintln!("comply: ignoring malformed {}", candidate.display());
                return None;
            }
        };
        let arc = Arc::new(manifest);
        if let Ok(mut map) = self.cargo_manifest_cache.lock() {
            map.entry(manifest_dir).or_insert_with(|| Arc::clone(&arc));
        }
        Some(arc)
    }

    /// True if a non-relative `spec` resolves to a local source file via the
    /// nearest tsconfig's `baseUrl` (e.g. `baseUrl: "."` turns `src/types/Foo`
    /// into `<tsconfig_dir>/src/types/Foo.ts`). Such imports are project files,
    /// not npm packages. Returns `false` when no `baseUrl` is configured or the
    /// candidate does not exist on disk, so genuine package imports still fire.
    pub fn resolves_via_tsconfig_base_url(&self, importer: &Path, spec: &str) -> bool {
        let Some(tsconfig_dir) = self.nearest_tsconfig_dir(importer) else {
            return false;
        };
        let Some(tsconfig) = self.nearest_tsconfig(importer) else {
            return false;
        };
        let Some(base_url) = tsconfig.base_url.as_ref() else {
            return false;
        };
        let candidate = tsconfig_dir.join(base_url).join(spec);
        local_source_exists(&candidate)
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
    pub fn workspace_package_names(&self) -> &[String] {
        self.workspace_package_names_cache.get_or_init(|| {
            self.workspace_roots
                .iter()
                .filter_map(|root| {
                    let raw = std::fs::read_to_string(root.join("package.json")).ok()?;
                    let pkg = PackageJson::parse(&raw)?;
                    pkg.name
                })
                .collect()
        })
    }

    /// True if `name` is declared as a dependency in *any* `package.json` under
    /// the project root tree of `importer` (excluding `node_modules`).
    ///
    /// Monorepos that manage packages without a `workspaces` field (e.g. nest)
    /// keep their shared (dev)dependencies in sibling `packages/*/package.json`
    /// manifests and hoist them at runtime. A file in a sibling directory with
    /// no manifest of its own (an `integration/` test tree) imports those
    /// packages legitimately, so `no-implicit-deps` consults the union of every
    /// declared dep across the tree before flagging. The set is built once per
    /// resolved root and memoized; a genuinely undeclared package is absent from
    /// it and still fires.
    pub fn dep_declared_in_tree(&self, importer: &Path, name: &str) -> bool {
        let Some(root) = self.tree_dep_root(importer) else {
            return false;
        };
        if let Some(hit) = self.tree_dep_names_cache.lock().unwrap().get(&root) {
            return hit.contains(name);
        }
        let names = Arc::new(collect_tree_dep_names(&root));
        let found = names.contains(name);
        self.tree_dep_names_cache
            .lock()
            .unwrap()
            .insert(root, names);
        found
    }

    /// True if `name` is declared as a dependency in any *member* package of the
    /// npm-workspaces root nearest to `importer`.
    ///
    /// npm hoists every workspace member's dependencies to the shared root
    /// `node_modules`, so a member may import a specifier declared only in a
    /// sibling member (e.g. `@jest/globals` declared in
    /// `packages/integration-testsuite` and imported from `packages/server`).
    /// Member directories are resolved from the root manifest's `workspaces`
    /// globs, so the check holds even when `project_root` is scoped to a single
    /// member (where the tree walk in [`dep_declared_in_tree`] never reaches the
    /// siblings). The aggregated dep set is built once per workspaces root and
    /// memoized; a specifier declared in no member is absent and still fires.
    pub fn dep_declared_in_workspace_siblings(&self, importer: &Path, name: &str) -> bool {
        let Some(root) = self.workspaces_root(importer) else {
            return false;
        };
        if let Some(hit) = self.workspace_sibling_deps_cache.lock().unwrap().get(&root) {
            return hit.contains(name);
        }
        let names = Arc::new(collect_workspace_member_deps(&root));
        let found = names.contains(name);
        self.workspace_sibling_deps_cache
            .lock()
            .unwrap()
            .insert(root, names);
        found
    }

    /// Directory of the nearest ancestor `package.json` (starting at `importer`)
    /// that declares a non-empty `workspaces` field, or that has a
    /// `pnpm-workspace.yaml` beside it — the workspaces root. Walks the chain of
    /// ancestor manifests, jumping to each manifest's parent directory, so a
    /// member manifest without `workspaces` does not stop the search. Bounded to
    /// 8 manifest hops to defend against a pathological tree. `None` when no
    /// ancestor manifest declares a workspace layout.
    fn workspaces_root(&self, importer: &Path) -> Option<PathBuf> {
        let mut probe = importer.join("_");
        for _ in 0..8 {
            let manifest_dir = self.nearest_package_json_dir(&probe)?;
            let declares_npm_workspaces = self
                .nearest_package_json(&probe)
                .is_some_and(|pkg| !pkg.workspaces.is_empty());
            if declares_npm_workspaces || manifest_dir.join("pnpm-workspace.yaml").is_file() {
                return Some(manifest_dir);
            }
            probe = manifest_dir.parent()?.join("_");
        }
        None
    }

    /// Resolve the project root used to scope [`dep_declared_in_tree`]: the
    /// explicit `project_root` when known, else the topmost ancestor directory
    /// of `importer` that owns a `package.json`.
    fn tree_dep_root(&self, importer: &Path) -> Option<PathBuf> {
        if let Some(root) = self.project_root.clone() {
            return Some(root);
        }
        let mut topmost: Option<PathBuf> = None;
        let mut dir = importer.parent();
        while let Some(d) = dir {
            if d.join("package.json").is_file() {
                topmost = Some(d.to_path_buf());
            }
            dir = d.parent();
        }
        topmost
    }

    /// Lazily-loaded set of Prisma model names (lowercase) that declare a
    /// `deletedAt` field in the project's `schema.prisma`. Returns `None` when
    /// no `schema.prisma` is found — callers should fire on all models in that
    /// case to preserve backward-compatible behaviour.
    pub fn prisma_soft_delete_models(&self) -> Option<&HashSet<String>> {
        self.prisma_soft_delete_models
            .get_or_init(|| {
                let start: PathBuf = self
                    .project_root
                    .clone()
                    .or_else(|| std::env::current_dir().ok())?;
                let schema_dir = walk_up_finding(&start, "schema.prisma")?;
                let schema =
                    std::fs::read_to_string(schema_dir.join("schema.prisma")).ok()?;
                Some(parse_prisma_soft_delete_models(&schema))
            })
            .as_ref()
    }

    #[cfg(test)]
    #[must_use]
    pub fn for_test_with_prisma_models(models: &[&str]) -> Self {
        let ctx = ProjectCtx::default();
        let set: HashSet<String> = models.iter().map(|s| s.to_lowercase()).collect();
        let _ = ctx.prisma_soft_delete_models.set(Some(set));
        ctx
    }
}

/// Parse a `schema.prisma` text and return the lowercase names of models that
/// declare a `deletedAt` field. Uses a simple line-based scan — no full Prisma
/// parser needed.
fn parse_prisma_soft_delete_models(schema: &str) -> HashSet<String> {
    let mut result = HashSet::new();
    let mut current_model: Option<String> = None;
    let mut block_has_deleted_at = false;
    let mut depth: i32 = 0;

    for line in schema.lines() {
        let trimmed = line.trim();

        if let Some(ref _name) = current_model {
            // Count brace depth to detect block end.
            for c in trimmed.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            if trimmed.contains("deletedAt") {
                block_has_deleted_at = true;
            }
            if depth == 0 {
                if block_has_deleted_at {
                    result.insert(current_model.take().unwrap().to_lowercase());
                } else {
                    current_model = None;
                }
                block_has_deleted_at = false;
            }
        } else if trimmed.starts_with("model ") {
            let rest = &trimmed["model ".len()..];
            let name = rest.split_whitespace().next().unwrap_or("");
            if name.is_empty() || name == "{" {
                continue;
            }
            current_model = Some(name.to_string());
            block_has_deleted_at = false;
            depth = 0;
            for c in trimmed.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
        }
    }
    result
}

/// Resolve workspace glob patterns to actual package directories.
/// Returns the list of workspace root directories that contain a `package.json`.
///
/// Member globs come from the root manifest's `workspaces` field; when that is
/// absent (pnpm monorepos declare members in `pnpm-workspace.yaml` instead) the
/// globs are read from `pnpm-workspace.yaml` beside the manifest. Each
/// `/`-separated segment is expanded against the filesystem: a literal segment
/// is joined directly, a `*` segment fans out to every subdirectory at that
/// level. Multi-level globs (e.g. `packages/auth-providers/*/*`) are fully
/// expanded so packages nested several directories deep are still discovered.
fn resolve_workspace_roots(project_root: Option<&Path>, pkg: &PackageJson) -> Vec<PathBuf> {
    let Some(root) = project_root else {
        return Vec::new();
    };
    let patterns = if pkg.workspaces.is_empty() {
        read_pnpm_workspace_globs(root)
    } else {
        pkg.workspaces.clone()
    };
    if patterns.is_empty() {
        return Vec::new();
    }

    let mut roots = Vec::new();
    for pattern in &patterns {
        for dir in expand_workspace_glob(root, pattern) {
            if dir.join("package.json").exists() {
                roots.push(dir);
            }
        }
    }
    roots
}

/// Read the workspace package globs from `pnpm-workspace.yaml` at `dir`.
///
/// pnpm monorepos declare members under a top-level `packages:` sequence here
/// instead of `package.json#workspaces`, so the globs feed the same
/// [`expand_workspace_glob`] machinery as the npm field. Negated patterns
/// (a leading `!`, pnpm's exclusion syntax) are dropped — they would otherwise
/// be expanded as if they selected members. Returns an empty list when the file
/// is absent, unparseable, or declares no `packages:` sequence.
fn read_pnpm_workspace_globs(dir: &Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(dir.join("pnpm-workspace.yaml")) else {
        return Vec::new();
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&raw) else {
        return Vec::new();
    };
    let Some(seq) = value.get("packages").and_then(|node| node.as_sequence()) else {
        return Vec::new();
    };
    seq.iter()
        .filter_map(|item| item.as_str())
        .filter(|pattern| !pattern.starts_with('!'))
        .map(String::from)
        .collect()
}

/// Expand a single workspace glob pattern into the directories it matches on
/// disk, descending one filesystem level per `*` segment.
fn expand_workspace_glob(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut current = vec![root.to_path_buf()];
    for segment in pattern.split('/').filter(|s| !s.is_empty()) {
        let mut next = Vec::new();
        if segment.contains('*') {
            for base in &current {
                if let Ok(entries) = std::fs::read_dir(base) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            next.push(path);
                        }
                    }
                }
            }
        } else {
            for base in &current {
                let path = base.join(segment);
                if path.is_dir() {
                    next.push(path);
                }
            }
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }
    current
}

/// Collect the union of every dependency name declared across all member
/// packages of the npm-workspaces root at `root`. The root manifest's
/// `workspaces` globs are expanded (via [`resolve_workspace_roots`]) to the
/// member directories, and each member's declared deps (plus `engines` keys) are
/// unioned. Only the members listed under `workspaces` are read — no full tree
/// walk — so the cost is bounded by the number of workspace packages.
fn collect_workspace_member_deps(root: &Path) -> HashSet<String> {
    let mut names = HashSet::new();
    let Some(pkg) = std::fs::read_to_string(root.join("package.json"))
        .ok()
        .and_then(|raw| PackageJson::parse(&raw))
    else {
        return names;
    };
    for member in resolve_workspace_roots(Some(root), &pkg) {
        if let Ok(raw) = std::fs::read_to_string(member.join("package.json"))
            && let Some(member_pkg) = PackageJson::parse(&raw)
        {
            names.extend(member_pkg.all_deps().map(str::to_string));
            names.extend(member_pkg.engines.keys().cloned());
        }
    }
    names
}

/// Collect the union of every dependency name declared in every `package.json`
/// under `root` (excluding `node_modules` and dot-directories), bounded by a
/// depth limit so a pathologically deep tree can't blow the stack or stall.
fn collect_tree_dep_names(root: &Path) -> HashSet<String> {
    const MAX_DEPTH: u32 = 8;
    let mut names = HashSet::new();
    let mut stack: Vec<(PathBuf, u32)> = vec![(root.to_path_buf(), 0)];

    while let Some((dir, depth)) = stack.pop() {
        if let Ok(raw) = std::fs::read_to_string(dir.join("package.json"))
            && let Some(pkg) = PackageJson::parse(&raw)
        {
            names.extend(pkg.all_deps().map(str::to_string));
            names.extend(pkg.engines.keys().cloned());
        }
        if depth >= MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skip = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_none_or(|n| n == "node_modules" || n.starts_with('.'));
            if skip {
                continue;
            }
            stack.push((path, depth + 1));
        }
    }
    names
}

/// Walk up from `path` to the nearest `filename`, returning a cached parse.
/// Cache miss: read + parse + insert at the manifest directory. Cache hit:
/// clone the `Arc` under the lock.
fn nearest<T>(
    cache: &Mutex<HashMap<PathBuf, Arc<T>>>,
    dir_cache: &Mutex<HashMap<&'static str, HashMap<PathBuf, Option<PathBuf>>>>,
    path: &Path,
    filename: &'static str,
    parse: impl Fn(&str) -> Option<T>,
) -> Option<Arc<T>> {
    let start_dir = path.parent()?;

    // Resolve the *nearest* manifest on disk first, then cache keyed by that
    // resolved dir. Caching by ancestor lookup instead would let a cached far
    // ancestor shadow a closer, not-yet-parsed manifest — the monorepo case of
    // a root tsconfig alongside per-package tsconfigs, where resolution order
    // is arbitrary.
    let manifest_dir = walk_up_finding_cached(dir_cache, start_dir, filename)?;

    if let Some(hit) = cache.lock().ok()?.get(&manifest_dir) {
        return Some(Arc::clone(hit));
    }

    let candidate = manifest_dir.join(filename);
    let raw = std::fs::read_to_string(&candidate).ok()?;
    let parsed = match parse(&raw) {
        Some(parsed) => parsed,
        None => {
            eprintln!("comply: ignoring malformed {}", candidate.display());
            return None;
        }
    };
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

/// Of two ancestor directories on the same root-to-leaf chain, the one closer
/// to the file — i.e. with more path components. Ties (equal depth, hence equal
/// directories) return `a`.
fn deeper_dir(a: PathBuf, b: PathBuf) -> PathBuf {
    if b.components().count() > a.components().count() {
        b
    } else {
        a
    }
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

/// [`walk_up_finding`] memoized by the per-run `manifest_dir_cache`. The walk
/// is deterministic for the duration of a run, so the memo is output-identical
/// while collapsing thousands of duplicate stat-walks (one per file sharing a
/// directory) down to one per (directory, marker).
fn walk_up_finding_cached(
    cache: &Mutex<HashMap<&'static str, HashMap<PathBuf, Option<PathBuf>>>>,
    start: &Path,
    target: &'static str,
) -> Option<PathBuf> {
    if let Ok(c) = cache.lock()
        && let Some(inner) = c.get(target)
        && let Some(hit) = inner.get(start)
    {
        return hit.clone();
    }
    let resolved = walk_up_finding(start, target);
    if let Ok(mut c) = cache.lock() {
        c.entry(target)
            .or_default()
            .insert(start.to_path_buf(), resolved.clone());
    }
    resolved
}

/// Extensions TypeScript appends when resolving an extension-less module
/// specifier. Mirrors the resolver's extension list so a `baseUrl` import like
/// `src/types/Foo` matches `Foo.ts`, `Foo/index.ts`, etc.
const TS_SOURCE_EXTENSIONS: &[&str] =
    &["ts", "tsx", "d.ts", "mts", "cts", "js", "jsx", "mjs", "cjs", "vue", "json"];

/// True if `candidate` (an extension-less module path) points at an existing
/// local source file — directly, with a TS/JS extension appended, or as a
/// directory containing an `index.*` entry.
fn local_source_exists(candidate: &Path) -> bool {
    if candidate.is_file() {
        return true;
    }
    if let Some(name) = candidate.file_name().and_then(|n| n.to_str()) {
        if let Some(parent) = candidate.parent() {
            for ext in TS_SOURCE_EXTENSIONS {
                if parent.join(format!("{name}.{ext}")).is_file() {
                    return true;
                }
            }
        }
    }
    if candidate.is_dir() {
        for ext in TS_SOURCE_EXTENSIONS {
            if candidate.join(format!("index.{ext}")).is_file() {
                return true;
            }
        }
    }
    false
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

/// Extract `lib.entryFile` from an `ng-package.json`'s raw text, normalized to
/// forward slashes with any leading `./` stripped so it joins cleanly onto the
/// manifest directory. Parsed via [`parse_jsonc`] because ng-packagr configs are
/// JSONC (comments and trailing commas appear, especially in secondary entry
/// points). Returns `None` when the text is unparseable or declares no string
/// `lib.entryFile`.
fn parse_ng_package_entry_file(raw: &str) -> Option<String> {
    let json = parse_jsonc(raw)?;
    let entry = json.get("lib")?.get("entryFile")?.as_str()?;
    let rel = entry.strip_prefix("./").unwrap_or(entry);
    if rel.is_empty() {
        return None;
    }
    Some(rel.replace('\\', "/"))
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
    use std::io::Read;
    let mut stripped = String::new();
    json_comments::StripComments::new(raw.as_bytes())
        .read_to_string(&mut stripped)
        .ok()?;
    serde_json::from_str(&strip_trailing_commas(&stripped)).ok()
}

/// Remove trailing commas (a `,` whose next non-whitespace character is `}` or
/// `]`) that JSONC and `tsconfig.json` permit but `serde_json` rejects. String
/// contents are preserved verbatim — commas inside string literals are never
/// touched.
fn strip_trailing_commas(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1; // skip the trailing comma
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
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
    fn min_major_version_reads_lowest_token() {
        assert_eq!(min_major_version(">=18.0.0"), Some(18));
        assert_eq!(min_major_version("^18 || ^19"), Some(18));
        assert_eq!(min_major_version("18.x"), Some(18));
        assert_eq!(min_major_version("^18.2.0"), Some(18));
        assert_eq!(min_major_version("~18.3"), Some(18));
        assert_eq!(min_major_version(">=19.0.0"), Some(19));
        assert_eq!(min_major_version("^19"), Some(19));
        assert_eq!(min_major_version("workspace:*"), None);
    }

    #[test]
    fn script_command_heads_pick_segment_binaries() {
        assert_eq!(
            extract_script_command_heads("changeset publish"),
            vec!["changeset"]
        );
        assert_eq!(
            extract_script_command_heads("pnpm -r build && manypkg check"),
            vec!["pnpm", "manypkg"]
        );
        assert_eq!(
            extract_script_command_heads("node_modules/.bin/eslint ."),
            vec!["eslint"]
        );
        // A leading flag names no binary; the empty trailing segment is dropped.
        assert!(extract_script_command_heads("--silent").is_empty());
    }

    #[test]
    fn cli_runner_candidates_derive_from_package_name() {
        assert_eq!(cli_runner_binary_candidates("@manypkg/cli"), vec!["manypkg"]);
        assert_eq!(
            cli_runner_binary_candidates("@changesets/cli"),
            vec!["changesets", "changeset"]
        );
        assert_eq!(
            cli_runner_binary_candidates("knip-cli"),
            vec!["knip-cli", "knip"]
        );
        // A plain library yields no candidates, so a coincidental script command
        // head can never exempt it.
        assert!(cli_runner_binary_candidates("lodash").is_empty());
        assert!(cli_runner_binary_candidates("@scope/utils").is_empty());
    }

    #[test]
    fn scripts_invoke_dep_binary_matches_runner_packages() {
        let pkg = PackageJson::parse(
            r#"{
                "name": "root",
                "scripts": { "release": "changeset publish", "check": "manypkg check" },
                "dependencies": { "@changesets/cli": "^2.0.0", "@manypkg/cli": "^0.21.0" }
            }"#,
        )
        .unwrap();
        assert!(pkg.scripts_invoke_dep_binary("@changesets/cli"));
        assert!(pkg.scripts_invoke_dep_binary("@manypkg/cli"));
        // A library dep whose binary no script runs is not exempted.
        assert!(!pkg.scripts_invoke_dep_binary("lodash"));
    }

    #[test]
    fn react_supports_v18_reads_react_range() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"lib","peerDependencies":{"react":">=18.0.0"}}"#,
        )
        .unwrap();
        let ctx = ProjectCtx::empty();
        assert!(ctx.react_supports_v18(&dir.path().join("t.tsx")));
    }

    #[test]
    fn react_supports_v18_false_for_react19_only() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"app","dependencies":{"react":"^19.0.0"}}"#,
        )
        .unwrap();
        let ctx = ProjectCtx::empty();
        assert!(!ctx.react_supports_v18(&dir.path().join("t.tsx")));
    }

    #[test]
    fn react_supports_v18_false_when_no_react_declared() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"app"}"#).unwrap();
        let ctx = ProjectCtx::empty();
        assert!(!ctx.react_supports_v18(&dir.path().join("t.tsx")));
    }

    #[test]
    fn is_ng_package_entry_file_matches_lib_entry_file() {
        let dir = TempDir::new().unwrap();
        let pkg_dir = dir.path().join("packages/angular-server");
        std::fs::create_dir_all(pkg_dir.join("src")).unwrap();
        std::fs::write(
            pkg_dir.join("ng-package.json"),
            r#"{ "lib": { "entryFile": "src/public_api.ts" } }"#,
        )
        .unwrap();
        let entry = pkg_dir.join("src/public_api.ts");
        std::fs::write(&entry, "export {};").unwrap();
        let other = pkg_dir.join("src/ionic-server-module.ts");
        std::fs::write(&other, "export class X {}").unwrap();

        let ctx = ProjectCtx::empty();
        assert!(ctx.is_ng_package_entry_file(&entry));
        assert!(!ctx.is_ng_package_entry_file(&other));
    }

    #[test]
    fn package_boundary_dir_prefers_nearest_ng_package_over_package_json() {
        let dir = TempDir::new().unwrap();
        let lib = dir.path().join("packages/angular");
        std::fs::create_dir_all(lib.join("common/src")).unwrap();
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::write(lib.join("package.json"), r#"{"name":"@ionic/angular"}"#).unwrap();
        std::fs::write(lib.join("ng-package.json"), r#"{"lib":{"entryFile":"src/index.ts"}}"#)
            .unwrap();
        std::fs::write(
            lib.join("common/ng-package.json"),
            r#"{"lib":{"entryFile":"src/index.ts"}}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        // A secondary entry point resolves to its own ng-package directory, not
        // the shared package.json directory.
        assert_eq!(
            ctx.package_boundary_dir(&lib.join("common/src/index.ts")),
            Some(lib.join("common"))
        );
        // The primary entry point resolves to the library root.
        assert_eq!(
            ctx.package_boundary_dir(&lib.join("src/index.ts")),
            Some(lib.clone())
        );
    }

    #[test]
    fn package_boundary_dir_falls_back_to_package_json() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"plain"}"#).unwrap();
        let ctx = ProjectCtx::empty();
        assert_eq!(
            ctx.package_boundary_dir(&dir.path().join("src/index.ts")),
            Some(dir.path().to_path_buf())
        );
    }

    #[test]
    fn is_ng_package_entry_file_false_without_ng_package_json() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let entry = dir.path().join("src/public_api.ts");
        std::fs::write(&entry, "export {};").unwrap();
        let ctx = ProjectCtx::empty();
        assert!(!ctx.is_ng_package_entry_file(&entry));
    }

    #[test]
    fn parse_ng_package_entry_file_reads_jsonc_with_trailing_comma() {
        assert_eq!(
            parse_ng_package_entry_file("{\n  \"lib\": {\n    \"entryFile\": \"src/index.ts\"\n  },\n}\n"),
            Some("src/index.ts".to_string())
        );
        assert_eq!(
            parse_ng_package_entry_file(r#"{ "lib": { "entryFile": "./src/public_api.ts" } }"#),
            Some("src/public_api.ts".to_string())
        );
        assert_eq!(parse_ng_package_entry_file(r#"{ "lib": {} }"#), None);
        assert_eq!(parse_ng_package_entry_file("not json"), None);
    }

    fn min_node(node_spec: &str) -> Option<(u32, u32)> {
        let raw = format!(r#"{{"engines":{{"node":"{node_spec}"}}}}"#);
        PackageJson::parse(&raw).unwrap().min_node_version()
    }

    #[test]
    fn min_node_version_parses_major_and_minor() {
        assert_eq!(min_node(">=20.11.0"), Some((20, 11)));
        assert_eq!(min_node("^21.2.0"), Some((21, 2)));
        assert_eq!(min_node(">=18"), Some((18, 0)));
        assert_eq!(min_node("12.20.0"), Some((12, 20)));
        assert_eq!(min_node("20.x"), Some((20, 0)));
    }

    #[test]
    fn min_node_version_takes_smallest_alternative() {
        assert_eq!(min_node(">=20.9 || >=18.18"), Some((18, 18)));
        assert_eq!(min_node(">=21.2 || >=20.11"), Some((20, 11)));
    }

    #[test]
    fn min_node_version_none_when_unconstrained() {
        assert_eq!(min_node("*"), None);
        assert_eq!(PackageJson::parse(r#"{"name":"t"}"#).unwrap().min_node_version(), None);
    }

    #[test]
    fn uses_vitest_true_for_vitest_dependency() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"app","devDependencies":{"vitest":"^1"}}"#,
        )
        .unwrap();
        let ctx = ProjectCtx::empty();
        assert!(ctx.uses_vitest(&dir.path().join("App.test.tsx")));
    }

    #[test]
    fn uses_vitest_true_for_vitest_test_script() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"app","scripts":{"test":"vitest run"}}"#,
        )
        .unwrap();
        let ctx = ProjectCtx::empty();
        assert!(ctx.uses_vitest(&dir.path().join("App.test.tsx")));
    }

    #[test]
    fn uses_vitest_true_when_only_vitest_config_present() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"app"}"#).unwrap();
        std::fs::write(dir.path().join("vitest.config.ts"), "export default {}").unwrap();
        let ctx = ProjectCtx::empty();
        assert!(ctx.uses_vitest(&dir.path().join("App.test.tsx")));
    }

    #[test]
    fn uses_vitest_false_for_jest_project() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"app","scripts":{"test":"jest"},"devDependencies":{"jest":"^29"}}"#,
        )
        .unwrap();
        let ctx = ProjectCtx::empty();
        assert!(!ctx.uses_vitest(&dir.path().join("App.test.tsx")));
    }

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
    fn tsconfig_parses_paths_with_trailing_commas() {
        // Regression #1060: tsconfig.json permits trailing commas (JSONC). They must
        // not break parsing, or path aliases silently disappear and bare imports get
        // wrongly flagged as implicit deps.
        let ts = Tsconfig::parse(
            "{\"compilerOptions\":{\"paths\":{\"@app\":[\"./app\"],}},\"exclude\":[\"node_modules\",]}",
        )
        .expect("tsconfig with trailing commas must parse");
        assert!(ts.paths.contains_key("@app"));
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
    fn is_package_entry_file_matches_declared_main() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"vue","main":"index.js"}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        assert!(ctx.is_package_entry_file(&dir.path().join("index.js")));
        assert!(!ctx.is_package_entry_file(&dir.path().join("other.js")));
    }

    #[test]
    fn is_package_entry_file_matches_exports_dot_targets() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"vue","exports":{".":{"import":"./index.mjs","require":"./index.cjs"}}}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        assert!(ctx.is_package_entry_file(&dir.path().join("index.mjs")));
        assert!(ctx.is_package_entry_file(&dir.path().join("index.cjs")));
        assert!(!ctx.is_package_entry_file(&dir.path().join("other.js")));
    }

    #[test]
    fn is_script_entry_file_matches_scripts_invocation() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@redwoodjs/auth-azure-web","scripts":{"build":"tsx ./build.ts"},"main":"./dist/cjs/index.js"}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        // The file `scripts.build` runs directly is a script entry.
        assert!(ctx.is_script_entry_file(&dir.path().join("build.ts")));
        // A sibling library module the scripts never invoke is not.
        assert!(!ctx.is_script_entry_file(&dir.path().join("src/load.ts")));
    }

    #[test]
    fn is_in_script_entry_dir_covers_sibling_helpers_of_a_script_entry() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"recharts","scripts":{"omnidoc":"tsx ./omnidoc/generateApiDoc.ts"},"main":"./lib/index.js","files":["lib"]}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        // The script entry itself is in a script-entry directory.
        assert!(ctx.is_in_script_entry_dir(&dir.path().join("omnidoc/generateApiDoc.ts")));
        // A sibling helper the entry imports but no script names directly is too.
        assert!(ctx.is_in_script_entry_dir(&dir.path().join("omnidoc/readProject.ts")));
        // Published source in src/ is not in the toolchain directory.
        assert!(!ctx.is_in_script_entry_dir(&dir.path().join("src/index.ts")));
    }

    #[test]
    fn is_in_script_entry_dir_does_not_mark_the_manifest_root() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"pkg","scripts":{"build":"tsx ./build.ts"},"main":"./index.js"}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        // A root-level script entry must not turn the manifest root — where
        // published source also lives — into a tooling directory.
        assert!(!ctx.is_in_script_entry_dir(&dir.path().join("index.ts")));
        assert!(!ctx.is_in_script_entry_dir(&dir.path().join("build.ts")));
    }

    #[test]
    fn is_declared_entry_barrel_matches_source_by_exports_subpath_stem() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"framer-motion","exports":{".":{"import":"./dist/es/index.mjs"},"./dom":{"import":"./dist/es/dom.mjs"}}}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        // Source barrels carry the stem of the built artifact each subpath ships.
        assert!(ctx.is_declared_entry_barrel(&dir.path().join("src/index.ts")));
        assert!(ctx.is_declared_entry_barrel(&dir.path().join("src/dom.ts")));
        assert!(!ctx.is_declared_entry_barrel(&dir.path().join("src/internal.ts")));
    }

    #[test]
    fn is_bundled_build_input_true_for_src_with_entries_outside_src() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"monaco-editor","main":"./min/x.js","module":"./esm/x.js"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src/features")).unwrap();

        let ctx = ProjectCtx::empty();
        assert!(ctx.is_bundled_build_input(&dir.path().join("src/features/register.js")));
    }

    #[test]
    fn is_bundled_build_input_false_when_entry_under_src() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"ships-src","main":"./src/index.js"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();

        let ctx = ProjectCtx::empty();
        assert!(!ctx.is_bundled_build_input(&dir.path().join("src/util.js")));
    }

    #[test]
    fn is_bundled_build_input_false_outside_src() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"monaco-editor","main":"./min/x.js"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();

        let ctx = ProjectCtx::empty();
        assert!(!ctx.is_bundled_build_input(&dir.path().join("lib/feature.js")));
    }

    #[test]
    fn is_bundled_build_input_false_for_non_library() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"some-app"}"#).unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();

        let ctx = ProjectCtx::empty();
        assert!(!ctx.is_bundled_build_input(&dir.path().join("src/feature.js")));
    }

    #[test]
    fn nearest_prefers_closer_manifest_over_cached_ancestor() {
        // Root tsconfig with no paths; a nested package tsconfig with an alias.
        // Resolving a file under the root first caches the root dir. Resolving
        // a file in the nested package must still return the *closer* tsconfig,
        // not the cached ancestor.
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"compilerOptions":{}}"#,
        )
        .unwrap();
        let pkg = dir.path().join("packages").join("app");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(
            pkg.join("tsconfig.json"),
            r#"{"compilerOptions":{"paths":{"~/*":["./src/*"]}}}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        let root_ts = ctx.nearest_tsconfig(&dir.path().join("root.ts")).unwrap();
        assert!(root_ts.alias_prefixes().is_empty());

        let pkg_ts = ctx.nearest_tsconfig(&pkg.join("src").join("t.ts")).unwrap();
        assert_eq!(pkg_ts.alias_prefixes(), vec!["~".to_string()]);
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
        let mut names = ctx.workspace_package_names().to_vec();
        names.sort();
        assert_eq!(
            names,
            vec!["@scope/bar".to_string(), "@scope/foo".to_string()]
        );
    }

    #[test]
    fn resolves_multi_level_workspace_glob() {
        // Regression for #1685: redwood-style root manifest with a two-level
        // glob (`packages/auth-providers/*/*`). The real packages live two
        // directories below `auth-providers`, so a single-level expansion misses
        // them and their entry points get flagged as unused.
        let dir = TempDir::new().unwrap();
        let manifest =
            r#"{"name":"root","workspaces":["packages/*","packages/auth-providers/*/*"]}"#;
        std::fs::write(dir.path().join("package.json"), manifest).unwrap();

        // Single-level package under packages/*.
        let cli = dir.path().join("packages").join("cli");
        std::fs::create_dir_all(&cli).unwrap();
        std::fs::write(cli.join("package.json"), r#"{"name":"@redwoodjs/cli"}"#).unwrap();

        // Two-level packages under packages/auth-providers/*/*.
        let web = dir
            .path()
            .join("packages")
            .join("auth-providers")
            .join("azureActiveDirectory")
            .join("web");
        let setup = dir
            .path()
            .join("packages")
            .join("auth-providers")
            .join("azureActiveDirectory")
            .join("setup");
        std::fs::create_dir_all(&web).unwrap();
        std::fs::create_dir_all(&setup).unwrap();
        std::fs::write(web.join("package.json"), r#"{"name":"@rw/azure-web"}"#).unwrap();
        std::fs::write(setup.join("package.json"), r#"{"name":"@rw/azure-setup"}"#).unwrap();

        let pkg = PackageJson::parse(manifest).unwrap();
        let roots = resolve_workspace_roots(Some(dir.path()), &pkg);

        let ctx = ProjectCtx {
            workspace_roots: roots,
            ..ProjectCtx::default()
        };
        let mut names = ctx.workspace_package_names().to_vec();
        names.sort();
        assert_eq!(
            names,
            vec![
                "@redwoodjs/cli".to_string(),
                "@rw/azure-setup".to_string(),
                "@rw/azure-web".to_string(),
            ]
        );
    }

    // Regression #1671: when `project_root` is scoped to one workspace member
    // (e.g. comply run on `packages/server`), the tree walk in
    // `dep_declared_in_tree` never reaches sibling members, but
    // `dep_declared_in_workspace_siblings` resolves them from the root
    // `workspaces` globs and recognizes a dep declared only in a sibling.
    #[test]
    fn workspace_sibling_dep_found_when_root_scoped_to_member() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        let testsuite = dir.path().join("packages").join("integration-testsuite");
        std::fs::create_dir_all(&testsuite).unwrap();
        std::fs::write(
            testsuite.join("package.json"),
            r#"{"name":"@scope/testsuite","peerDependencies":{"@jest/globals":"29.x || 30.x"}}"#,
        )
        .unwrap();
        let server = dir.path().join("packages").join("server");
        let tests = server.join("src").join("__tests__");
        std::fs::create_dir_all(&tests).unwrap();
        std::fs::write(server.join("package.json"), r#"{"name":"@scope/server"}"#).unwrap();
        let importer = tests.join("errors.test.ts");
        std::fs::write(&importer, "import { it } from '@jest/globals';").unwrap();

        // Load with only the member file so `project_root` resolves to the member
        // package — the configuration that drove the real apollo-server FP.
        use crate::files::Language;
        let source_file = SourceFile {
            path: importer.clone(),
            language: Language::TypeScript,
        };
        let ctx = ProjectCtx::load(&[&source_file], &Config::default());
        assert_eq!(ctx.project_root.as_deref(), Some(server.as_path()));
        assert!(
            ctx.dep_declared_in_workspace_siblings(&importer, "@jest/globals"),
            "sibling-member dep must resolve via the root workspaces globs"
        );
        assert!(
            !ctx.dep_declared_in_workspace_siblings(&importer, "totally-undeclared-pkg"),
            "a dep in no member must not resolve"
        );
    }

    #[test]
    fn resolves_workspace_packages_object_form_issue_1601() {
        // Yarn Berry / pnpm nested-object form: `"workspaces": {"packages": [...]}`.
        let dir = TempDir::new().unwrap();
        let manifest = r#"{"name":"xstate-monorepo","workspaces":{"packages":["packages/*","scripts/*"]}}"#;
        std::fs::write(dir.path().join("package.json"), manifest).unwrap();
        let core = dir.path().join("packages").join("core");
        std::fs::create_dir_all(&core).unwrap();
        std::fs::write(core.join("package.json"), r#"{"name":"xstate"}"#).unwrap();

        let pkg = PackageJson::parse(manifest).unwrap();
        assert_eq!(
            pkg.workspaces,
            vec!["packages/*".to_string(), "scripts/*".to_string()]
        );
        let roots = resolve_workspace_roots(Some(dir.path()), &pkg);
        assert_eq!(roots, vec![core]);
    }

    #[test]
    fn workspaces_object_without_packages_key_returns_empty() {
        // The object form may carry only `nohoist`; with no `packages` array
        // there are no globs to discover.
        let pkg = PackageJson::parse(r#"{"name":"root","workspaces":{"nohoist":["**/foo"]}}"#)
            .unwrap();
        assert!(pkg.workspaces.is_empty());
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

    // Regression #1797: pnpm monorepos (wagmi) declare members in
    // `pnpm-workspace.yaml`, leaving `package.json#workspaces` empty. The
    // members must still resolve so a cross-workspace import is recognized.
    #[test]
    fn resolves_pnpm_workspace_packages() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"workspace","private":true}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("pnpm-workspace.yaml"),
            "packages:\n  - packages/*\n  - playgrounds/*\n  - '!**/dist/**'\n",
        )
        .unwrap();
        let connectors = dir.path().join("packages").join("connectors");
        let test_pkg = dir.path().join("packages").join("test");
        std::fs::create_dir_all(&connectors).unwrap();
        std::fs::create_dir_all(&test_pkg).unwrap();
        std::fs::write(
            connectors.join("package.json"),
            r#"{"name":"@wagmi/connectors"}"#,
        )
        .unwrap();
        std::fs::write(test_pkg.join("package.json"), r#"{"name":"@wagmi/test"}"#).unwrap();

        let pkg = PackageJson::parse(r#"{"name":"workspace","private":true}"#).unwrap();
        let roots = resolve_workspace_roots(Some(dir.path()), &pkg);
        assert_eq!(roots.len(), 2);

        let ctx = ProjectCtx {
            workspace_roots: roots,
            ..ProjectCtx::default()
        };
        let mut names = ctx.workspace_package_names().to_vec();
        names.sort();
        assert_eq!(
            names,
            vec!["@wagmi/connectors".to_string(), "@wagmi/test".to_string()]
        );
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
    fn exact_optional_property_types_parsed_directly() {
        let ts = Tsconfig::parse(r#"{"compilerOptions":{"exactOptionalPropertyTypes":true}}"#)
            .unwrap();
        assert!(ts.exact_optional_property_types);
    }

    #[test]
    fn exact_optional_property_types_defaults_false() {
        let ts = Tsconfig::parse(r#"{"compilerOptions":{"strict":true}}"#).unwrap();
        assert!(!ts.exact_optional_property_types);
    }

    #[test]
    fn exact_optional_property_types_inherited_through_extends() {
        // Regression #2075 (zod case): the flag lives in the extended base
        // config; a child that extends it and omits the flag must still inherit
        // `true`, so `?: T | undefined` is not wrongly flagged as redundant.
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("tsconfig.base.json"),
            r#"{"compilerOptions":{"exactOptionalPropertyTypes":true}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"extends":"./tsconfig.base.json","compilerOptions":{"strict":true}}"#,
        )
        .unwrap();
        let ts = Tsconfig::load(dir.path()).unwrap();
        assert!(ts.exact_optional_property_types);
    }

    #[test]
    fn uses_exact_optional_property_types_predicate() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"exactOptionalPropertyTypes":true}}"#,
        )
        .unwrap();
        let ctx = ProjectCtx::empty();
        assert!(ctx.uses_exact_optional_property_types(&dir.path().join("src.ts")));
    }

    #[test]
    fn uses_exact_optional_property_types_false_without_flag() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"strict":true}}"#,
        )
        .unwrap();
        let ctx = ProjectCtx::empty();
        assert!(!ctx.uses_exact_optional_property_types(&dir.path().join("src.ts")));
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

    fn load_ctx_in(dir: &TempDir) -> ProjectCtx {
        use crate::files::{Language, SourceFile};
        let file_path = dir.path().join("app.tsx");
        std::fs::write(&file_path, "export const x = 1;").unwrap();
        let source_file = SourceFile {
            path: file_path,
            language: Language::Tsx,
        };
        ProjectCtx::load(&[&source_file], &Config::default())
    }

    #[test]
    fn uses_tailwind_true_with_config_file() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"x"}"#).unwrap();
        std::fs::write(dir.path().join("tailwind.config.ts"), "export default {};").unwrap();
        assert!(load_ctx_in(&dir).uses_tailwind());
    }

    #[test]
    fn uses_tailwind_true_with_dependency() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"x","devDependencies":{"tailwindcss":"^4"}}"#,
        )
        .unwrap();
        assert!(load_ctx_in(&dir).uses_tailwind());
    }

    #[test]
    fn uses_tailwind_true_with_scoped_plugin() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"x","dependencies":{"@tailwindcss/vite":"^4"}}"#,
        )
        .unwrap();
        assert!(load_ctx_in(&dir).uses_tailwind());
    }

    #[test]
    fn uses_tailwind_false_without_config_or_dependency() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"x","dependencies":{"antd":"^5"}}"#,
        )
        .unwrap();
        assert!(!load_ctx_in(&dir).uses_tailwind());
    }

    #[test]
    fn nearest_cargo_manifest_walks_up_caches_and_classifies() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"
[package]
name = "mytool"
version = "0.1.0"
edition = "2021"
categories = ["no-std"]

[[bin]]
name = "mytool"
path = "src/main.rs"

[dependencies]
tokio = "1"
"#,
        )
        .unwrap();
        let nested = dir.path().join("src").join("deep");
        std::fs::create_dir_all(&nested).unwrap();

        let ctx = ProjectCtx::empty();
        let first = ctx.nearest_cargo_manifest(&nested.join("t.rs")).unwrap();
        let second = ctx.nearest_cargo_manifest(&nested.join("other.rs")).unwrap();
        assert!(
            Arc::ptr_eq(&first, &second),
            "sibling files should share the same cached Arc"
        );
        assert!(
            first.is_binary_only(),
            "no [lib] table and no src/lib.rs on disk => binary-only"
        );
        assert!(first.has_async_runtime(), "tokio is declared");
        assert!(first.is_no_std(), "categories lists no-std");
    }
}
