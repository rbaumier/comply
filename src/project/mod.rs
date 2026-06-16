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
    /// `imports` field entries paired with their manifest-relative string target,
    /// e.g. `("#app/*", "./app/*")`. Conditional-object targets contribute their
    /// first string condition value (`{ "import": "./x.js" }` → `"./x.js"`); a
    /// non-string target (nested arrays/objects) is skipped. Lets the import
    /// resolver map a `#`-prefixed specifier to its physical path independent of
    /// `node_modules`/tsconfig resolution, which can silently fail when a
    /// checked-in `tsconfig.json` `extends` an absent package.
    pub subpath_import_targets: Vec<(String, String)>,
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
    /// True if the package declares `"private": true` — it is never published to
    /// npm. The `dependencies`/`devDependencies` distinction only matters for
    /// published packages whose consumers `npm install` them and need runtime
    /// deps in `dependencies`; for a private package everything is bundled at
    /// build time, so importing from `devDependencies` is correct.
    pub is_private: bool,
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
    /// Manifest-dir-relative `exports` targets that contain a `*` wildcard — e.g.
    /// `src/v4/locales/*` from `"./v4/locales/*": { ... }`. Gathered from every
    /// condition (standard and non-standard like `@zod/source`), since a package
    /// may point only a non-standard condition at its `.ts` source while standard
    /// `import`/`types` point at compiled output. Each pattern is a glob whose `*`
    /// expands to any substring, so every source file matching it is a public
    /// entry point reachable across the package boundary
    /// (`import("mylib/v4/locales/de")`) and never imported within the repo.
    /// Stored separately from [`entry_files`] because the literal-equality entry
    /// check cannot match a path against a glob.
    ///
    /// [`entry_files`]: PackageJson::entry_files
    pub entry_wildcards: BTreeSet<String>,
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
    /// Entries of the `files` field — the npm publish whitelist of paths and
    /// directory globs shipped to the registry. Stored manifest-dir-relative,
    /// forward-slash, leading `./` stripped; a directory entry keeps its
    /// trailing `/` so a consumer can tell a file path (`index.js`) apart from a
    /// published subtree (`lib/`). A pre-`exports`-era CJS library
    /// (e.g. express 5.x) declares only `files` plus npm's default `index.js`
    /// entry, so this whitelist is the package's published surface when no
    /// `main`/`exports`/`module` exists.
    pub files: BTreeSet<String>,
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
            subpath_import_targets: collect_subpath_import_targets(&json),
            has_browserslist: json.get("browserslist").is_some(),
            is_library: json.get("main").is_some()
                || json.get("exports").is_some()
                || json.get("module").is_some(),
            has_bin: json.get("bin").is_some(),
            is_private: parse_private(&json),
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
            entry_wildcards: collect_entry_wildcards(&json),
            entries_outside_src: entries_outside_src(&json),
            export_entry_stems: collect_export_entry_stems(&json),
            files: collect_files_whitelist(&json),
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

    /// True when this manifest declares none of the fields that make a
    /// `package.json` a real package boundary: no `name`, no dependency section
    /// (`dependencies`/`devDependencies`/`peerDependencies`/`optionalDependencies`),
    /// no published surface (`main`/`exports`/`module` via [`is_library`], or
    /// `bin` via [`has_bin`]), and no `workspaces`. Such a file (typically just
    /// `{"type":"module"}` in an ESM subtree) only configures Node's module
    /// system; it neither declares dependencies nor a public API. Package
    /// resolution treats it as transparent and continues up to the nearest
    /// substantive manifest.
    ///
    /// [`is_library`]: PackageJson::is_library
    /// [`has_bin`]: PackageJson::has_bin
    pub fn is_marker_only(&self) -> bool {
        self.name.is_none()
            && self.dependencies.is_empty()
            && self.dev_dependencies.is_empty()
            && self.peer_dependencies.is_empty()
            && self.optional_dependencies.is_empty()
            && !self.is_library
            && !self.has_bin
            && self.workspaces.is_empty()
    }

    /// True when this manifest is a private test/harness overlay rather than a
    /// standalone package boundary: `"private": true` with no `workspaces`
    /// field. Such a manifest (e.g. a `test/package.json` declaring only its
    /// extra fixtures' deps) is never published and is not a workspace root; its
    /// file set belongs to the surrounding package, whose runtime dependencies
    /// the overlay's files may import. Dependency resolution therefore unions the
    /// parent manifest's declared deps on top of the overlay's own (see
    /// [`ProjectCtx::effective_package_jsons`]).
    ///
    /// A non-private manifest is a real, publishable package whose files do not
    /// inherit parent deps; a private manifest *with* `workspaces` is itself a
    /// workspace root, not an overlay — both return `false`.
    ///
    /// [`ProjectCtx::effective_package_jsons`]: crate::project::ProjectCtx::effective_package_jsons
    pub fn is_private_overlay(&self) -> bool {
        self.is_private && self.workspaces.is_empty()
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

/// The relative paths this package declares as its own entry points: the `main`
/// value plus every `exports` target — the `.` subpath and every other subpath
/// (e.g. `./inputrules`), each including its conditional `import`/`require`/
/// `default` variants. A package that publishes a library as a set of subpath
/// exports (e.g. `@tiptap/pm` exposing `@tiptap/pm/inputrules` →
/// `./inputrules/index.ts`) makes each target file a real entry point, reachable
/// only through the package boundary and never `import`ed within the repo. A
/// string `exports` (no subpath map) is itself the `.` target. Also includes the
/// `browser` and `react-native` substitute targets — the browser/native build of
/// the library that bundlers swap in at build time, reachable only through the
/// substitution map, never `import`ed.
fn collect_entry_files(json: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Some(main) = json.get("main").and_then(Value::as_str)
        && let Some(rel) = normalize_main_path(main)
    {
        out.insert(rel);
    }
    if let Some(exports) = json.get("exports") {
        collect_export_targets(exports, &mut out);
    }
    if let Some(browser) = json.get("browser") {
        collect_substitute_targets(browser, &mut out);
    }
    if let Some(native) = json.get("react-native") {
        collect_substitute_targets(native, &mut out);
    }
    out
}

/// The `exports` targets of `json` that contain a `*` wildcard, gathered from
/// every condition (standard and non-standard). Each is a glob pattern whose
/// `*` expands to any substring; a source file matching it is a public entry
/// point. Patterns whose target is the package root (`*` alone, after
/// normalization) are dropped — they would match every file in the package.
fn collect_entry_wildcards(json: &Value) -> BTreeSet<String> {
    let Some(exports) = json.get("exports") else {
        return BTreeSet::new();
    };
    let mut all = BTreeSet::new();
    collect_export_targets(exports, &mut all);
    all.into_iter()
        .filter(|target| target.contains('*') && target != "*")
        .collect()
}

/// Whether the manifest-relative source path `rel` matches a single-`*`
/// `exports` wildcard `pattern` (e.g. `src/locales/*` or `dist/*.js`). Per the
/// Node spec a subpath pattern carries exactly one `*` that expands to an
/// arbitrary substring, so the match is `rel == prefix + <non-empty> + suffix`:
/// `rel` starts with the text before `*`, ends with the text after it, the two
/// don't overlap, and the spanned substring is non-empty. Both sides use
/// forward slashes (the pattern is normalized, `rel` is path-derived on the
/// caller). A pattern with no `*` never reaches here.
fn wildcard_target_matches(pattern: &str, rel: &str) -> bool {
    let Some((prefix, suffix)) = pattern.split_once('*') else {
        return false;
    };
    rel.len() > prefix.len() + suffix.len()
        && rel.starts_with(prefix)
        && rel.ends_with(suffix)
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

/// The entries of the `files` field — the npm publish whitelist. Each is a
/// relative path or directory glob normalized to forward slashes with a leading
/// `./` stripped; a directory entry keeps its trailing `/` so a consumer can
/// distinguish a single published file from a published subtree. Negation
/// patterns (`!…`) and bare-glob wildcards are dropped: they exclude rather than
/// publish, or name no concrete path/subtree to match against.
fn collect_files_whitelist(json: &Value) -> BTreeSet<String> {
    json.get("files")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .filter(|entry| !entry.starts_with('!') && !entry.contains('*'))
                .filter_map(normalize_files_entry)
                .collect()
        })
        .unwrap_or_default()
}

/// Normalize one `files` entry: strip a leading `./`, convert backslashes to
/// forward slashes, and preserve a trailing `/` that marks a directory subtree.
/// Returns `None` for an empty or root (`.`) entry, which names no concrete
/// published path.
fn normalize_files_entry(entry: &str) -> Option<String> {
    let rel = entry.strip_prefix("./").unwrap_or(entry).replace('\\', "/");
    if rel.is_empty() || rel == "." || rel == "/" {
        return None;
    }
    Some(rel)
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

/// Pair each `imports` field key with its string target. A value may be a plain
/// string (`"#app/*": "./app/*"`) or a conditions object whose values are
/// strings (`"#app/*": { "default": "./app/*" }`); for the object form one string
/// condition value is taken. Nested conditions (object/array values) and `null`
/// targets are skipped — they have no single physical target.
fn collect_subpath_import_targets(json: &Value) -> Vec<(String, String)> {
    let Some(obj) = json.get("imports").and_then(Value::as_object) else {
        return Vec::new();
    };
    obj.iter()
        .filter_map(|(key, target)| {
            subpath_import_string_target(target).map(|t| (key.clone(), t.to_string()))
        })
        .collect()
}

/// Resolve one `imports` target node to a single string path. A bare string is
/// returned as-is; a conditions object yields its first string condition value.
/// Returns `None` for nested/array/null targets.
fn subpath_import_string_target(target: &Value) -> Option<&str> {
    match target {
        Value::String(s) => Some(s.as_str()),
        Value::Object(conditions) => conditions.values().find_map(Value::as_str),
        _ => None,
    }
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

/// True when `package.json` declares `"private": true`. npm honours the boolean
/// form; the stringified `"true"` some tooling writes is accepted too. Any other
/// shape (absent, `false`, an object) reads as publishable.
fn parse_private(json: &Value) -> bool {
    match json.get("private") {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s == "true",
        _ => false,
    }
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

/// Named exports the Vitest runtime invokes on a `globalSetup` module by
/// convention — `setup`/`teardown` (and the default export, which Vitest also
/// accepts as the setup function). None of them has a static importer.
const VITEST_GLOBAL_SETUP_EXPORTS: &[&str] = &["setup", "teardown", "default"];

/// Exports a Cloudflare Worker module-format entry point exposes to the Workers
/// runtime: the `default` export object plus the lifecycle handlers the runtime
/// invokes on it (`fetch`, `scheduled`, `queue`, `email`, `tail`). The runtime
/// resolves the entry module from `wrangler.toml` and calls these by name, so
/// none of them ever has a static importer.
pub const CLOUDFLARE_WORKER_HANDLER_EXPORTS: &[&str] =
    &["default", "fetch", "scheduled", "queue", "email", "tail"];

/// Lifecycle-handler names whose presence on the `export default` object
/// identifies a Cloudflare Worker entry module. `fetch` is the canonical HTTP
/// handler; the others cover the cron, queue-consumer, email, and tail-worker
/// triggers. One match is enough to recognize the shape.
const CLOUDFLARE_WORKER_HANDLER_TRIGGERS: &[&str] =
    &["fetch", "scheduled", "queue", "email", "tail"];

/// True when `source` is a Cloudflare Worker module-format entry point: it has an
/// `export default` whose value is an object literal carrying at least one of the
/// Workers lifecycle handlers (`fetch`/`scheduled`/`queue`/`email`/`tail`). The
/// Cloudflare runtime consumes this default export by resolving the entry from
/// `wrangler.toml` and calling the handlers by name, never through a static
/// import, so dead-export must not flag it.
///
/// Keying on the export *shape* rather than a filename is deliberate: worker
/// entry files are not conventionally named, and the default-object-with-`fetch`
/// shape is specific enough to identify the convention on its own — an ordinary
/// `export default {}` with no lifecycle handler stays subject to the rule.
#[must_use]
pub fn is_cloudflare_worker_entry_source(source: &str, lang: crate::files::Language) -> bool {
    let Some(grammar) = crate::parsing::ts_language_for(lang) else {
        return false;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return false;
    }
    let Some(tree) = parser.parse(source, None) else {
        return false;
    };
    let bytes = source.as_bytes();
    let mut found = false;
    crate::rules::walker::walk_tree(&tree, |node| {
        if found || node.kind() != "export_statement" {
            return;
        }
        let is_default = node.children(&mut node.walk()).any(|c| c.kind() == "default");
        if !is_default {
            return;
        }
        let Some(object) = node
            .named_children(&mut node.walk())
            .find(|c| c.kind() == "object")
        else {
            return;
        };
        if object_has_cloudflare_worker_handler(object, bytes) {
            found = true;
        }
    });
    found
}

/// True when `object` (a tree-sitter `object` node) declares a property named
/// after a Cloudflare Worker lifecycle handler, in any of the forms an entry
/// module uses: a method (`async fetch(req, env) {}`), a `key: value` pair
/// (`fetch: handler`), or a shorthand (`{ fetch }`).
fn object_has_cloudflare_worker_handler(object: tree_sitter::Node, source: &[u8]) -> bool {
    object.named_children(&mut object.walk()).any(|member| {
        let name = match member.kind() {
            "method_definition" | "pair" => member
                .named_children(&mut member.walk())
                .find(|c| c.kind() == "property_identifier")
                .and_then(|n| n.utf8_text(source).ok()),
            "shorthand_property_identifier" => member.utf8_text(source).ok(),
            _ => None,
        };
        name.is_some_and(|n| CLOUDFLARE_WORKER_HANDLER_TRIGGERS.contains(&n))
    })
}

/// Module specifier the OXLint plugin API is imported from. A file is only
/// recognized as a plugin entry when its `definePlugin` comes from this package,
/// so an unrelated local `definePlugin` never matches the shape.
const OXLINT_PLUGINS_MODULE: &str = "@oxlint/plugins";

/// Factory call whose result an OXLint plugin file exports as `default`.
const OXLINT_DEFINE_PLUGIN: &str = "definePlugin";

/// Export name an OXLint plugin file exposes to the linter at run time: the
/// `default` export carries the plugin definition. OXLint resolves plugin
/// modules from its config (`oxlintrc.json`) and loads this default export
/// itself, so it never has a static importer.
pub const OXLINT_PLUGIN_ENTRY_EXPORTS: &[&str] = &["default"];

/// True when `source` is an OXLint custom-plugin entry point: it imports
/// `definePlugin` from `@oxlint/plugins` and its `export default` value is a
/// call to `definePlugin(...)`. OXLint resolves plugin modules from its config
/// and consumes this default export at run time, never through a static import,
/// so dead-export must not flag it.
///
/// Both signals are required — the `@oxlint/plugins` import source *and* the
/// `export default definePlugin(...)` call shape — so an unrelated `definePlugin`
/// from another module, or a default export that merely happens to be a call,
/// does not match. An ordinary `export default {}` stays subject to the rule.
#[must_use]
pub fn is_oxlint_plugin_entry_source(source: &str, lang: crate::files::Language) -> bool {
    let Some(grammar) = crate::parsing::ts_language_for(lang) else {
        return false;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return false;
    }
    let Some(tree) = parser.parse(source, None) else {
        return false;
    };
    let bytes = source.as_bytes();
    let mut imports_define_plugin = false;
    let mut exports_define_plugin_default = false;
    crate::rules::walker::walk_tree(&tree, |node| {
        match node.kind() {
            "import_statement" if imports_from_oxlint_plugins(node, bytes) => {
                imports_define_plugin = true;
            }
            "export_statement" if export_default_calls_define_plugin(node, bytes) => {
                exports_define_plugin_default = true;
            }
            _ => {}
        }
    });
    imports_define_plugin && exports_define_plugin_default
}

/// True when `node` (an `import_statement`) imports from `@oxlint/plugins`.
/// Keys on the module specifier alone — the `definePlugin` binding is matched on
/// the export side — so any import from the package satisfies the source gate.
fn imports_from_oxlint_plugins(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.child_by_field_name("source")
        .and_then(|src| src.utf8_text(source).ok())
        .map(|spec| spec.trim_matches(|c| c == '\'' || c == '"' || c == '`'))
        .is_some_and(|spec| spec == OXLINT_PLUGINS_MODULE)
}

/// True when `node` (an `export_statement`) is an `export default` whose value
/// is a call to `definePlugin(...)`.
fn export_default_calls_define_plugin(node: tree_sitter::Node, source: &[u8]) -> bool {
    let is_default = node.children(&mut node.walk()).any(|c| c.kind() == "default");
    if !is_default {
        return false;
    }
    node.named_children(&mut node.walk())
        .filter(|c| c.kind() == "call_expression")
        .any(|call| {
            call.child_by_field_name("function")
                .and_then(|f| f.utf8_text(source).ok())
                .is_some_and(|name| name == OXLINT_DEFINE_PLUGIN)
        })
}

/// Export name a TSLint custom-rule file exposes to the TSLint runtime: the
/// `Rule` class. TSLint discovers rule modules by directory and loads them with
/// `require()`, then instantiates `new Rule()` — the class name is the plugin
/// API contract — so `Rule` never has a static importer yet is live. Used as the
/// cheap name gate before the shape-confirming source scan runs.
pub const TSLINT_RULE_ENTRY_EXPORTS: &[&str] = &["Rule"];

/// Base class a TSLint custom rule extends, as the rightmost segment of the
/// heritage clause — `AbstractRule` (imported directly) or `Rules.AbstractRule` /
/// `Lint.Rules.AbstractRule` (reached through the `tslint` namespace).
const TSLINT_ABSTRACT_RULE: &str = "AbstractRule";

/// True when `source` is a TSLint custom-rule module: it imports from `tslint`
/// (the bare specifier or a `tslint/<subpath>` such as `tslint/lib/rules`) and
/// declares a class named `Rule` that extends `AbstractRule` (or
/// `Rules.AbstractRule` / `Lint.Rules.AbstractRule`). TSLint discovers rule files
/// by directory and instantiates `new Rule()` at run time, never through a static
/// import, so the `Rule` export is live despite having no importer.
///
/// Both signals are required — the `tslint` import source *and* the `Rule extends
/// AbstractRule` class shape — so an ordinary `export class Rule {}`, or a `Rule`
/// extending a local non-tslint `AbstractRule`, stays subject to the rule.
#[must_use]
pub fn is_tslint_rule_source(source: &str, lang: crate::files::Language) -> bool {
    let Some(grammar) = crate::parsing::ts_language_for(lang) else {
        return false;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return false;
    }
    let Some(tree) = parser.parse(source, None) else {
        return false;
    };
    let bytes = source.as_bytes();
    let mut imports_tslint = false;
    let mut declares_rule_class = false;
    crate::rules::walker::walk_tree(&tree, |node| match node.kind() {
        "import_statement" if imports_from_tslint(node, bytes) => {
            imports_tslint = true;
        }
        "class_declaration" if is_tslint_rule_class(node, bytes) => {
            declares_rule_class = true;
        }
        _ => {}
    });
    imports_tslint && declares_rule_class
}

/// True when `node` (an `import_statement`) imports from the `tslint` package —
/// the bare `tslint` specifier or a `tslint/<subpath>` (e.g. `tslint/lib/rules`).
fn imports_from_tslint(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.child_by_field_name("source")
        .and_then(|src| src.utf8_text(source).ok())
        .map(|spec| spec.trim_matches(|c| c == '\'' || c == '"' || c == '`'))
        .is_some_and(|spec| spec == "tslint" || spec.starts_with("tslint/"))
}

/// True when `node` (a `class_declaration`) is named `Rule` and extends a base
/// whose rightmost name segment is `AbstractRule` — `AbstractRule`,
/// `Rules.AbstractRule`, or `Lint.Rules.AbstractRule`.
fn is_tslint_rule_class(node: tree_sitter::Node, source: &[u8]) -> bool {
    let named_rule = node
        .named_children(&mut node.walk())
        .find(|c| c.kind() == "type_identifier" || c.kind() == "identifier")
        .and_then(|id| id.utf8_text(source).ok())
        .is_some_and(|name| name == "Rule");
    if !named_rule {
        return false;
    }
    node.named_children(&mut node.walk())
        .filter(|c| c.kind() == "class_heritage")
        .any(|heritage| heritage_extends_abstract_rule(heritage, source))
}

/// True when a `class_heritage` node has an `extends` clause whose base type's
/// rightmost name segment is `AbstractRule`. A `member_expression` /
/// `nested_type_identifier` (`Rules.AbstractRule`) is matched on its last segment;
/// a bare `identifier`/`type_identifier` (`AbstractRule`) on its own text.
fn heritage_extends_abstract_rule(heritage: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = heritage.walk();
    for clause in heritage.named_children(&mut cursor) {
        if clause.kind() != "extends_clause" {
            continue;
        }
        let extends_abstract_rule = clause
            .named_children(&mut clause.walk())
            .any(|base| rightmost_name_is_abstract_rule(base, source));
        if extends_abstract_rule {
            return true;
        }
    }
    false
}

/// True when `node`'s rightmost name segment equals `AbstractRule`. Handles a
/// bare `identifier`/`type_identifier` and a dotted `member_expression` /
/// `nested_type_identifier` (the heritage base is a value expression in JS, a
/// type reference in TS), where the last `.`-segment is the relevant name.
fn rightmost_name_is_abstract_rule(node: tree_sitter::Node, source: &[u8]) -> bool {
    let text = match node.kind() {
        "identifier" | "type_identifier" | "member_expression" | "nested_type_identifier" => {
            node.utf8_text(source).ok()
        }
        // A generic instantiation (`AbstractRule<T>`) wraps the base name in a
        // `generic_type` whose first child carries the name.
        "generic_type" => node
            .named_children(&mut node.walk())
            .next()
            .and_then(|n| n.utf8_text(source).ok()),
        _ => None,
    };
    text.and_then(|t| t.rsplit('.').next())
        .is_some_and(|last| last == TSLINT_ABSTRACT_RULE)
}

/// Export names a k6 load-test script exposes to the k6 runtime: the required
/// `default` entry function, the `options` runtime configuration, and the
/// `setup`/`teardown` lifecycle hooks. The k6 CLI loads the script and calls
/// these by name, never through a static import, so none of them has an importer.
pub const K6_SCRIPT_MAGIC_EXPORTS: &[&str] = &["default", "options", "setup", "teardown"];

/// True when `source` is a k6 load-test script: it imports from the `k6` runtime
/// module (`k6` itself or a `k6/<subpath>` such as `k6/http`) and has an
/// `export default` (k6's required entry point). The k6 CLI resolves the script,
/// reads its `options` export, and invokes `default`/`setup`/`teardown` by name,
/// never through a static import, so those exports are live despite having no
/// importer.
///
/// Both signals are required — the `k6`/`k6/*` import source *and* the
/// `export default` — so an ordinary module that merely imports from a `k6`-like
/// path, or one that has an `export default` without the runtime import, does not
/// match. Keying on this shape rather than a directory name is deliberate: k6
/// scripts follow no naming convention.
#[must_use]
pub fn is_k6_script_source(source: &str, lang: crate::files::Language) -> bool {
    let Some(grammar) = crate::parsing::ts_language_for(lang) else {
        return false;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return false;
    }
    let Some(tree) = parser.parse(source, None) else {
        return false;
    };
    let bytes = source.as_bytes();
    let mut imports_k6 = false;
    let mut has_default_export = false;
    crate::rules::walker::walk_tree(&tree, |node| {
        match node.kind() {
            "import_statement" if imports_from_k6_runtime(node, bytes) => {
                imports_k6 = true;
            }
            "export_statement"
                if node.children(&mut node.walk()).any(|c| c.kind() == "default") =>
            {
                has_default_export = true;
            }
            _ => {}
        }
    });
    imports_k6 && has_default_export
}

/// True when `node` (an `import_statement`) imports from the k6 runtime module —
/// the bare `k6` specifier or a `k6/<subpath>` (e.g. `k6/http`, `k6/metrics`).
fn imports_from_k6_runtime(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.child_by_field_name("source")
        .and_then(|src| src.utf8_text(source).ok())
        .map(|spec| spec.trim_matches(|c| c == '\'' || c == '"' || c == '`'))
        .is_some_and(|spec| spec == "k6" || spec.starts_with("k6/"))
}

/// Module specifiers that mark a file as Convex backend code: the public
/// `convex/server` API and the per-project generated `convex/_generated/server`
/// re-export. A file is only recognized as a Convex function module when its
/// wrapper bindings come from one of these, so an unrelated local `query` /
/// `mutation` never matches the shape.
const CONVEX_SERVER_MODULES: &[&str] = &["convex/server", "convex/_generated/server"];

/// Convex wrapper functions whose call result a backend module exports. The
/// Convex deployment registers each export as a backend function and the
/// generated `api.*` types call them by path, never through a static import, so
/// `export const foo = query({...})` (and the internal variants) has no importer
/// yet is live.
const CONVEX_FUNCTION_WRAPPERS: &[&str] = &[
    "query",
    "mutation",
    "action",
    "internalQuery",
    "internalMutation",
    "internalAction",
];

/// Convex schema factory whose call a backend module exports as `default`.
/// `convex/schema.ts` exports `defineSchema(...)`, consumed by Convex's code
/// generator, never through a static import.
const CONVEX_DEFINE_SCHEMA: &str = "defineSchema";

/// Names of the exports a Convex backend module exposes to the Convex
/// deployment runtime, or an empty set when `source` is not a Convex backend
/// module. The Convex CLI deploys these and the generated `api.*`/`internal.*`
/// types reference them by path, never through a static import, so each has no
/// importer yet is live.
///
/// Two signals are required for the module to count: it must import from
/// `convex/server` (or `convex/_generated/server`) *and* expose at least one
/// Convex-shaped export — a `default` export of `defineSchema(...)` or a named
/// `export const X = query(...)/mutation(...)/action(...)` (or the internal
/// variants). The import gate keeps an unrelated local `query`/`mutation` from
/// matching. The returned set is scoped tightly to those wrapper-call exports
/// plus the `defineSchema` default, so a plain `export const helper = 5` in a
/// Convex module is not exempted.
///
/// Keying on the export *shape* rather than the `convex/` directory name is
/// deliberate: the directory is configurable and the wrapper-call shape
/// identifies the convention on its own.
#[must_use]
pub fn convex_magic_exports_for_source(
    source: &str,
    lang: crate::files::Language,
) -> HashSet<String> {
    let mut names = HashSet::new();
    let Some(grammar) = crate::parsing::ts_language_for(lang) else {
        return names;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return names;
    }
    let Some(tree) = parser.parse(source, None) else {
        return names;
    };
    let bytes = source.as_bytes();
    let mut imports_convex_server = false;
    crate::rules::walker::walk_tree(&tree, |node| match node.kind() {
        "import_statement" if imports_from_convex_server(node, bytes) => {
            imports_convex_server = true;
        }
        "export_statement" => {
            collect_convex_wrapper_exports(node, bytes, &mut names);
        }
        _ => {}
    });
    if imports_convex_server {
        names
    } else {
        HashSet::new()
    }
}

/// True when `node` (an `import_statement`) imports from a Convex server module.
/// Keys on the module specifier alone — the wrapper bindings are matched on the
/// export side — so any import from `convex/server` or `convex/_generated/server`
/// satisfies the source gate.
fn imports_from_convex_server(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.child_by_field_name("source")
        .and_then(|src| src.utf8_text(source).ok())
        .map(|spec| spec.trim_matches(|c| c == '\'' || c == '"' || c == '`'))
        .is_some_and(|spec| CONVEX_SERVER_MODULES.contains(&spec))
}

/// Push the names of any Convex-shaped exports declared by `node` (an
/// `export_statement`) into `out`: `default` when its value is a
/// `defineSchema(...)` call, and any named `export const X = <wrapper>(...)`
/// whose initializer calls one of `CONVEX_FUNCTION_WRAPPERS`.
fn collect_convex_wrapper_exports(
    node: tree_sitter::Node,
    source: &[u8],
    out: &mut HashSet<String>,
) {
    let is_default = node.children(&mut node.walk()).any(|c| c.kind() == "default");
    if is_default {
        let calls_define_schema = node
            .named_children(&mut node.walk())
            .filter(|c| c.kind() == "call_expression")
            .any(|call| call_callee_is(call, source, &|name| name == CONVEX_DEFINE_SCHEMA));
        if calls_define_schema {
            out.insert("default".to_string());
        }
        return;
    }
    let Some(decl) = node
        .named_children(&mut node.walk())
        .find(|c| c.kind() == "lexical_declaration" || c.kind() == "variable_declaration")
    else {
        return;
    };
    for declarator in decl
        .named_children(&mut decl.walk())
        .filter(|c| c.kind() == "variable_declarator")
    {
        let Some(value) = declarator.child_by_field_name("value") else {
            continue;
        };
        if value.kind() != "call_expression"
            || !call_callee_is(value, source, &|name| CONVEX_FUNCTION_WRAPPERS.contains(&name))
        {
            continue;
        }
        if let Some(name) = declarator
            .child_by_field_name("name")
            .filter(|n| n.kind() == "identifier")
            .and_then(|n| n.utf8_text(source).ok())
        {
            out.insert(name.to_string());
        }
    }
}

/// True when `call` (a `call_expression`) has a plain-identifier callee whose
/// text satisfies `pred`. A member-expression callee (`obj.query(...)`) does not
/// match, so only the bare Convex wrapper calls are recognized.
fn call_callee_is(
    call: tree_sitter::Node,
    source: &[u8],
    pred: &dyn Fn(&str) -> bool,
) -> bool {
    call.child_by_field_name("function")
        .filter(|f| f.kind() == "identifier")
        .and_then(|f| f.utf8_text(source).ok())
        .is_some_and(pred)
}

/// Export names a Node.js ESM customization-hook module exposes to the Node
/// runtime: the `resolve`/`load` chained hooks and the `globalPreload` hook.
/// Node loads the module via the `--loader`/`--import` (or `register(...)`) CLI
/// machinery and invokes these by name, never through a static import, so each
/// has no importer yet is live. Used as the cheap name gate before the
/// shape-confirming source scan runs.
pub const NODE_LOADER_HOOK_EXPORTS: &[&str] = &["resolve", "load", "globalPreload"];
/// Canonical last-parameter name of the chained `resolve` hook — the
/// `nextResolve` continuation Node passes so a hook can defer to the next one in
/// the chain. Its presence as the final parameter is the loader-hook fingerprint.
const NODE_RESOLVE_NEXT_PARAM: &str = "nextResolve";
/// Canonical last-parameter name of the chained `load` hook — the `nextLoad`
/// continuation, the `load` counterpart of `nextResolve`.
const NODE_LOAD_NEXT_PARAM: &str = "nextLoad";

/// Names of the Node.js ESM loader hooks `source` exposes with the canonical
/// chained-hook signature, or an empty set when `source` declares none. Node
/// loads a customization-hooks module through `--loader`/`--import` (or a
/// `register(...)` call) and invokes `resolve`/`load`/`globalPreload` by name,
/// never through a static import, so each has no importer yet is live.
///
/// The shape gate is deliberately strict because `resolve`/`load` are extremely
/// common export names: a `resolve` or `load` export only counts when its value
/// is a function whose *last* parameter is the chained-hook continuation
/// (`nextResolve` / `nextLoad`). `globalPreload` is included only when the module
/// also declares a shape-valid `resolve` or `load`, so a lone `globalPreload`
/// elsewhere is not exempted. The caller additionally gates on a loader-hook file
/// convention, so an ordinary `export const resolve = (x) => x` stays flagged.
#[must_use]
pub fn node_loader_hook_exports_for_source(
    source: &str,
    lang: crate::files::Language,
) -> HashSet<String> {
    let mut names = HashSet::new();
    let Some(grammar) = crate::parsing::ts_language_for(lang) else {
        return names;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return names;
    }
    let Some(tree) = parser.parse(source, None) else {
        return names;
    };
    let bytes = source.as_bytes();
    let mut has_global_preload = false;
    crate::rules::walker::walk_tree(&tree, |node| {
        if node.kind() != "export_statement" {
            return;
        }
        for (name, last_param) in exported_function_signatures(node, bytes) {
            match name.as_str() {
                "resolve" if last_param.as_deref() == Some(NODE_RESOLVE_NEXT_PARAM) => {
                    names.insert(name);
                }
                "load" if last_param.as_deref() == Some(NODE_LOAD_NEXT_PARAM) => {
                    names.insert(name);
                }
                "globalPreload" => has_global_preload = true,
                _ => {}
            }
        }
    });
    // `globalPreload` rides on a shape-valid `resolve`/`load` in the same module:
    // its own signature is too generic to identify the convention alone.
    if has_global_preload && (names.contains("resolve") || names.contains("load")) {
        names.insert("globalPreload".to_string());
    }
    names
}

/// `(exported name, last-parameter name)` for each named function-valued export
/// `node` (an `export_statement`) declares. Covers `export const f = (…) => {}` /
/// `= function (…) {}` and `export function f(…) {}`. The last-parameter name is
/// the final `required_parameter` / `optional_parameter` / `identifier` in the
/// function's `formal_parameters`, or `None` when it takes no parameters. Loader
/// hooks are always named exports, so the default-export form is not handled.
fn exported_function_signatures(
    node: tree_sitter::Node,
    source: &[u8],
) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    // `export function f(…) {}` — the declaration carries both name and params.
    if let Some(func) = node
        .named_children(&mut node.walk())
        .find(|c| c.kind() == "function_declaration" || c.kind() == "generator_function_declaration")
    {
        if let Some(name) = func
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
        {
            out.push((name.to_string(), last_param_name(func, source)));
        }
        return out;
    }
    // `export const f = (…) => {}` / `= function (…) {}` — name on the
    // declarator, params on its function-valued initializer.
    if let Some(decl) = node
        .named_children(&mut node.walk())
        .find(|c| c.kind() == "lexical_declaration" || c.kind() == "variable_declaration")
    {
        for declarator in decl
            .named_children(&mut decl.walk())
            .filter(|c| c.kind() == "variable_declarator")
        {
            let (Some(name), Some(value)) = (
                declarator
                    .child_by_field_name("name")
                    .filter(|n| n.kind() == "identifier")
                    .and_then(|n| n.utf8_text(source).ok()),
                declarator.child_by_field_name("value"),
            ) else {
                continue;
            };
            if matches!(value.kind(), "arrow_function" | "function_expression") {
                out.push((name.to_string(), last_param_name(value, source)));
            }
        }
    }
    out
}

/// Name of the last formal parameter of `func` (a function-shaped node), or
/// `None` when it declares no parameters. Reads the final named child of the
/// `formal_parameters` list, unwrapping a `required_parameter`/`optional_parameter`
/// to its inner pattern identifier.
fn last_param_name(func: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let params = func.child_by_field_name("parameters")?;
    let last = params.named_children(&mut params.walk()).last()?;
    let ident = match last.kind() {
        "required_parameter" | "optional_parameter" => last
            .child_by_field_name("pattern")
            .filter(|p| p.kind() == "identifier")?,
        "identifier" => last,
        _ => return None,
    };
    ident.utf8_text(source).ok().map(str::to_string)
}

/// True when the `globalSetup` option in a Vitest/Vite config's `raw` text
/// references `target`. `config_dir` is the directory holding the config, used
/// to resolve the relative specifiers the option carries.
///
/// `globalSetup` accepts a single path or an array of paths; both are quoted
/// string literals. The scan collects the quoted specifiers that follow the
/// `globalSetup:` key on its declaration span (up to the line's end or the
/// closing `]` of an array), resolves each relative to `config_dir`, and reports
/// a match when one resolves to `target`. Specifier resolution tolerates an
/// omitted extension and an `index` file, mirroring module resolution.
fn config_global_setup_references(raw: &str, config_dir: &Path, target: &Path) -> bool {
    global_setup_value_spans(raw).any(|span| {
        quoted_string_literals(span).any(|spec| specifier_resolves_to(config_dir, spec, target))
    })
}

/// Each value span of a `globalSetup` option in `raw`: from just after a
/// `globalSetup:` key to the end of its line, extended through a closing `]`
/// when the value opens an array literal. A `globalSetup` substring not
/// immediately followed by `:` (e.g. `globalSetupReady`) is skipped, so a
/// look-alike key never shadows a real one later in the file.
fn global_setup_value_spans(raw: &str) -> impl Iterator<Item = &str> {
    const KEY: &str = "globalSetup";
    let mut search_from = 0usize;
    std::iter::from_fn(move || {
        while let Some(rel) = raw[search_from..].find(KEY) {
            let key_at = search_from + rel;
            let after_key = &raw[key_at + KEY.len()..];
            search_from = key_at + KEY.len();
            // Require `:` directly after the key (only whitespace between),
            // ruling out an incidental substring such as `globalSetupReady`.
            let Some(colon) = after_key.find(':') else {
                continue;
            };
            if after_key[..colon].chars().any(|c| !c.is_whitespace()) {
                continue;
            }
            let value = &after_key[colon + 1..];
            let line_end = value.find('\n').unwrap_or(value.len());
            // An array value can span lines; extend the span to its closing `]`.
            let end = match value[..line_end].find('[') {
                Some(_) => value.find(']').map_or(line_end, |b| b + 1),
                None => line_end,
            };
            return Some(&value[..end]);
        }
        None
    })
}

/// Iterator over the contents of single-, double-, or backtick-quoted string
/// literals in `text`. Quote characters must match to close; escapes inside are
/// not interpreted (config path specifiers contain none).
fn quoted_string_literals(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let open = rest.find(['\'', '"', '`'])?;
        let quote = rest.as_bytes()[open] as char;
        let after_open = &rest[open + 1..];
        let close = after_open.find(quote)?;
        let literal = &after_open[..close];
        rest = &after_open[close + 1..];
        Some(literal)
    })
}

/// True when the module specifier `spec`, resolved relative to `config_dir`,
/// refers to `target`. Compares with `target`'s extension stripped so a
/// `'./global-setup'` (no extension) or `'./global-setup.ts'` both match a
/// `global-setup.ts` target; also handles a directory specifier resolving to its
/// `index` file.
fn specifier_resolves_to(config_dir: &Path, spec: &str, target: &Path) -> bool {
    let resolved = lexical_normalize(&config_dir.join(spec));
    let target = lexical_normalize(target);
    let resolved_stem = strip_module_extension(&resolved);
    let target_stem = strip_module_extension(&target);
    resolved == target
        || resolved_stem == target_stem
        || resolved_stem.join("index") == target_stem
}

/// `path` with `.` components dropped and `..` components collapsed against the
/// preceding segment, without touching the filesystem. Lets a config specifier
/// (`'./global-setup.ts'`) compare equal to the target's stored path
/// (`<dir>/global-setup.ts`), whose components carry no `.`/`..`.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// `path` with a single trailing JS/TS module extension removed, if present.
fn strip_module_extension(path: &Path) -> PathBuf {
    const MODULE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"];
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if MODULE_EXTENSIONS.contains(&ext) => path.with_extension(""),
        _ => path.to_path_buf(),
    }
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
    /// `compilerOptions.jsxImportSource` — the package the JSX factory is
    /// imported from when files use automatic-runtime JSX without an explicit
    /// import. `"react"` (or absent) means React; any other value (e.g.
    /// `"@builder.io/qwik"`, `"solid-js"`, `"preact"`) means a framework that
    /// uses native HTML attribute names in JSX.
    pub jsx_import_source: Option<String>,
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
            jsx_import_source: co
                .and_then(|x| x.get("jsxImportSource"))
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
        jsx_import_source: co
            .and_then(|x| x.get("jsxImportSource"))
            .and_then(|s| s.as_str())
            .map(String::from),
        out_dir: co
            .and_then(|x| x.get("outDir"))
            .and_then(|s| s.as_str())
            .map(PathBuf::from),
    }
}

/// Overlay `child` onto `parent`. Scalars (`base_url`, `module`,
/// `module_resolution`, `jsx`, `jsx_import_source`, `out_dir`) are taken from the
/// child when present;
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
        jsx_import_source: child.jsx_import_source.or(parent.jsx_import_source),
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
    /// One or more `[[bin]]` tables are present.
    has_bin_table: bool,
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

        let has_bin_table = value.get("bin").is_some();

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
            has_bin_table,
            async_runtime,
            no_std_category,
        })
    }

    /// True when the crate builds no library target: no `[lib]` table and no
    /// `src/lib.rs` next to the manifest.
    pub fn is_binary_only(&self) -> bool {
        !self.has_lib_table && !self.manifest_dir.join("src/lib.rs").is_file()
    }

    /// True when the crate builds at least one binary target: a `[[bin]]`
    /// table is declared, or `src/main.rs` exists next to the manifest. Unlike
    /// [`is_binary_only`], this stays true for application crates (e.g. CLIs)
    /// that also carry a `[lib]` purely to share code between their own
    /// binaries — those crates still own their stdout.
    ///
    /// [`is_binary_only`]: CargoManifest::is_binary_only
    pub fn declares_binary(&self) -> bool {
        self.has_bin_table || self.manifest_dir.join("src/main.rs").is_file()
    }

    /// True when the crate builds a library target: a `[lib]` table is declared,
    /// or `src/lib.rs` exists next to the manifest. The inverse of
    /// [`is_binary_only`] — a crate is a library when it exposes a `[lib]`
    /// target that downstream consumers depend on.
    ///
    /// [`is_binary_only`]: CargoManifest::is_binary_only
    pub fn declares_library(&self) -> bool {
        self.has_lib_table || self.manifest_dir.join("src/lib.rs").is_file()
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

    // "Does this crate's root declare `#![no_std]`?", keyed by crate (manifest)
    // directory. The crate root (`src/lib.rs` / `src/main.rs`) is read once per
    // crate rather than once per file, since every file in the crate shares the
    // same answer.
    crate_no_std_cache: Mutex<HashMap<PathBuf, bool>>,

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

    // "Does this package directory declare a Bazel `ng_package` target?", keyed
    // by the manifest directory. Angular's source packages carry a placeholder
    // `package.json` with no `main`/`exports`/`module` (those fields are emitted
    // by Bazel's `ng_package` rule into the build output), so content-only
    // library detection misclassifies them as apps. The sibling `BUILD.bazel`
    // declaring `ng_package(...)` is the source-tree library marker. Read lazily
    // on first miss and memoized — the answer is identical for every file under
    // the same package directory.
    bazel_ng_package_cache: Mutex<HashMap<PathBuf, bool>>,

    // Absolute directories declared as a Prisma `generator { output = … }` in
    // each `schema.prisma`, keyed by that schema's directory. The generated
    // client lands here at `prisma generate` time; the directory is gitignored
    // and absent in a clean checkout, so imports resolving into it are expected
    // to be unresolved at lint time. Resolved lazily on first miss and memoized
    // by schema dir — every importer under the same schema shares the answer. An
    // empty `Vec` caches a missing schema or one with no `output` (default
    // `node_modules/.prisma/client`, already covered by the build-output match).
    prisma_output_dirs_cache: Mutex<HashMap<PathBuf, Arc<Vec<PathBuf>>>>,

    // Distribution root of a shadcn-style component registry — the common
    // ancestor directory of every file a `registry.json` manifest declares,
    // keyed by the directory the upward `registry.json` walk started from. The
    // files under it are source artifacts the registry CLI copies into a user's
    // project, never imported as modules within the repo, so they have no
    // in-repo importer. Resolved lazily on first miss and memoized — `None`
    // caches a directory with no enclosing shadcn registry so the disk walk and
    // manifest parse run at most once per directory.
    registry_root_cache: Mutex<HashMap<PathBuf, Option<PathBuf>>>,
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
            "express", "hono", "elysia", "nestjs", "nextjs", "remix", "nuxt",
        ];
        if self
            .detected_frameworks
            .iter()
            .any(|f| HTTP_SERVER_FRAMEWORKS.contains(&f.name.as_str()))
        {
            return true;
        }
        // The `svelte` framework def is detected on both the bare `svelte`
        // compiler package and a SvelteKit app. Only SvelteKit serves HTTP
        // routes, and those come from `@sveltejs/kit`; the bare `svelte`
        // compiler is a build tool with no HTTP boundary. Gate the SvelteKit
        // classification on that package directly rather than on the shared
        // `svelte` framework name.
        self.package_json
            .as_ref()
            .is_some_and(|pkg| pkg.has_dep_or_engine("@sveltejs/kit"))
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
        if self.is_vitest_global_setup_file(path) {
            names.extend(VITEST_GLOBAL_SETUP_EXPORTS.iter().copied());
        }
        if self.is_docusaurus_for_path(path)
            && (crate::rules::path_utils::is_docusaurus_theme_swizzle(path)
                || crate::rules::path_utils::is_docusaurus_plugin_entry(path))
        {
            names.insert("default");
        }
        names
    }

    /// True when Docusaurus owns `path` — detected either at the project root or
    /// via the nearest `package.json` (a docs site nested in a monorepo package,
    /// e.g. `packages/website/`, is invisible to root-anchored detection).
    fn is_docusaurus_for_path(&self, path: &Path) -> bool {
        self.has_framework("docusaurus")
            || self
                .frameworks_for_path(path)
                .iter()
                .any(|f| f.name == "docusaurus")
    }

    /// True when Nuxt owns `path` — detected either at the project root or via
    /// the nearest `package.json` (a Nuxt app nested in a monorepo package, e.g.
    /// `docs/`, is invisible to root-anchored detection).
    pub fn is_nuxt_for_path(&self, path: &Path) -> bool {
        self.has_framework("nuxt")
            || self
                .frameworks_for_path(path)
                .iter()
                .any(|f| f.name == "nuxt")
    }

    /// Add a framework's route-scoped magic exports when `path` matches the file
    /// convention that consumes them. SvelteKit reserves `load`/`ssr`/`csr`/… in
    /// `+page`/`+layout`/`+server` route files and `match` in `src/params/*`;
    /// Vue Router reserves `parser` in `src/params/*`; Remix reserves
    /// `loader`/`action`/`meta`/… in `app/routes/*` modules; Nuxt reserves
    /// `default` in `server/api/*` and `server/routes/*` Nitro route modules. The
    /// router calls each by exact name, so they have no importer but are live.
    /// Each framework's `route_files` apply only when `path` matches that
    /// framework's own route-file convention, keeping a same-named export in an
    /// ordinary module flaggable.
    fn extend_route_magic_exports<'a>(&'a self, path: &Path, names: &mut HashSet<&'a str>) {
        let basename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_sveltekit_route = crate::rules::path_utils::is_sveltekit_route_file(basename);
        let is_param_matcher = crate::rules::path_utils::is_param_dir_file(path);
        let is_remix_route = crate::rules::path_utils::is_remix_route_file(path);
        let is_rr_root = crate::rules::path_utils::is_react_router_root_module(path);
        let is_rr_config = crate::rules::path_utils::is_react_router_routes_config(path);
        let is_astro_page = crate::rules::path_utils::is_astro_routed_page(path);
        let is_nuxt_server_route = crate::rules::path_utils::is_nuxt_server_route_file(path);
        if !is_sveltekit_route
            && !is_param_matcher
            && !is_remix_route
            && !is_rr_root
            && !is_rr_config
            && !is_astro_page
            && !is_nuxt_server_route
        {
            return;
        }
        // Only frameworks detected for this path (root manifest + nearest
        // package.json) contribute, so a non-SvelteKit `+page.ts` or a Remix-less
        // `app/routes/` file stays unaffected.
        let owning = self
            .detected_frameworks
            .iter()
            .copied()
            .chain(self.frameworks_for_path(path));
        for fw in owning {
            // `root.tsx`/`root.jsx` shares the route module export contract, so
            // it draws on the same `route_files` set (plus `Layout`).
            let route_file_match = match fw.name.as_str() {
                "svelte" => is_sveltekit_route,
                "remix" => is_remix_route || is_rr_root,
                "astro" => is_astro_page,
                "nuxt" => is_nuxt_server_route,
                _ => false,
            };
            if route_file_match {
                names.extend(fw.route_magic_exports.route_files.iter().map(String::as_str));
            }
            if is_param_matcher {
                names.extend(fw.route_magic_exports.param_matchers.iter().map(String::as_str));
            }
            if fw.name == "remix" && is_rr_config {
                names.extend(fw.route_magic_exports.config_files.iter().map(String::as_str));
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

    /// Walk up from `path` to the nearest *substantive* `package.json` and
    /// return the *directory* containing it. Marker-only manifests (see
    /// [`PackageJson::is_marker_only`]) are transparent — the walk continues
    /// past them to the nearest real package boundary, the same as
    /// [`nearest_package_json`]. The walk result is cached by start directory.
    ///
    /// [`nearest_package_json`]: ProjectCtx::nearest_package_json
    pub fn nearest_package_json_dir(&self, path: &Path) -> Option<PathBuf> {
        self.nearest_substantive_package_json(path)
            .map(|(dir, _)| dir)
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

    /// Walk up from `path` to the nearest *substantive* `package.json`, caching
    /// the parsed result by manifest directory. Marker-only manifests (see
    /// [`PackageJson::is_marker_only`]) — typically `{"type":"module"}` files
    /// that only flag an ESM subtree — are not package boundaries: the walk
    /// skips them and continues up to the nearest manifest that declares a
    /// `name`, dependencies, or a published surface. Returns the same `Arc` on
    /// repeated lookups against any file under the same resolved manifest.
    pub fn nearest_package_json(&self, path: &Path) -> Option<Arc<PackageJson>> {
        self.nearest_substantive_package_json(path)
            .map(|(_, pkg)| pkg)
    }

    /// Resolve the nearest *substantive* `package.json` for `path`, returning
    /// its directory paired with the parsed manifest. Starting from the nearest
    /// manifest on disk, marker-only manifests are skipped and the walk
    /// continues to the next ancestor manifest. If every ancestor is
    /// marker-only the nearest one is returned, so resolution never yields
    /// `None` when a `package.json` exists above `path`.
    ///
    /// Both [`nearest_package_json`] and [`nearest_package_json_dir`] project
    /// from this, so the directory and the parsed manifest always agree.
    ///
    /// [`nearest_package_json`]: ProjectCtx::nearest_package_json
    /// [`nearest_package_json_dir`]: ProjectCtx::nearest_package_json_dir
    fn nearest_substantive_package_json(
        &self,
        path: &Path,
    ) -> Option<(PathBuf, Arc<PackageJson>)> {
        let mut start_dir = path.parent()?.to_path_buf();
        let mut fallback: Option<(PathBuf, Arc<PackageJson>)> = None;

        // Bounded: deep ESM subtrees stack a handful of marker manifests at
        // most. The cap mirrors the ancestor-walk bound in `no-implicit-deps`.
        for _ in 0..8 {
            // No further manifest above the last marker: fall back to the
            // nearest one found rather than losing the boundary entirely.
            let Some(dir) =
                walk_up_finding_cached(&self.manifest_dir_cache, &start_dir, "package.json")
            else {
                break;
            };
            let Some(pkg) =
                nearest_parsed_at(&self.package_json_cache, &dir, "package.json", PackageJson::parse)
            else {
                break;
            };
            if !pkg.is_marker_only() {
                return Some((dir, pkg));
            }
            fallback.get_or_insert_with(|| (dir.clone(), Arc::clone(&pkg)));
            // Step above this marker manifest's directory and keep walking.
            let Some(parent) = dir.parent() else { break };
            start_dir = parent.to_path_buf();
        }

        fallback
    }

    /// The chain of `package.json` manifests whose declared dependencies are
    /// available to `path`, nearest first. Normally this is just the nearest
    /// substantive manifest. When that manifest is a private test/harness
    /// overlay (see [`PackageJson::is_private_overlay`]) the chain continues up
    /// to the parent package(s): the overlay's files belong to the surrounding
    /// package and may import its runtime dependencies, which the overlay's own
    /// thin `package.json` does not re-declare. The walk stops at the first
    /// substantive non-overlay manifest, so a real (non-private) package or a
    /// workspace root (`private` + `workspaces`) does not inherit parent deps.
    ///
    /// Dependency-membership rules (`unlisted-dependency`, `no-implicit-deps`)
    /// consult this chain instead of only the nearest manifest, so a parent
    /// dependency imported from a nested overlay is correctly resolved.
    ///
    /// [`PackageJson::is_private_overlay`]: PackageJson::is_private_overlay
    pub fn effective_package_jsons(&self, path: &Path) -> Vec<Arc<PackageJson>> {
        let mut chain = Vec::new();
        let Some((mut dir, mut pkg)) = self.nearest_substantive_package_json(path) else {
            return chain;
        };
        // Bounded: nested overlays are at most a couple deep. The cap mirrors
        // the ancestor-walk bound in `no-implicit-deps`.
        for _ in 0..8 {
            let is_overlay = pkg.is_private_overlay();
            chain.push(pkg);
            if !is_overlay {
                break;
            }
            // Resolve the parent package: the nearest substantive manifest above
            // this overlay's directory.
            let Some(parent) = dir.parent() else { break };
            let Some((parent_dir, parent_pkg)) =
                self.nearest_substantive_package_json(&parent.join("_"))
            else {
                break;
            };
            if parent_dir == dir {
                break;
            }
            dir = parent_dir;
            pkg = parent_pkg;
        }
        chain
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

    /// True when `path` matches a wildcard `exports` target of its nearest
    /// `package.json` — e.g. `src/v4/locales/de.ts` against the pattern
    /// `src/v4/locales/*`. A wildcard subpath export publishes every matching
    /// source file as a public entry point (`import("mylib/v4/locales/de")`),
    /// reachable only across the package boundary and never imported within the
    /// repo, so the import-graph BFS cannot reach it even though it is genuinely
    /// published — not dead code. The patterns are gathered from every condition
    /// (including non-standard ones like `@zod/source` that point at the `.ts`
    /// source while standard conditions point at compiled output). The `*` in a
    /// pattern matches any non-empty substring, matched here against the actual
    /// source path comply scans.
    pub fn is_wildcard_entry_file(&self, path: &Path) -> bool {
        let Some(manifest_dir) = self.nearest_package_json_dir(path) else {
            return false;
        };
        let Some(pkg) = self.nearest_package_json(path) else {
            return false;
        };
        let Some(rel) = path.strip_prefix(manifest_dir).ok().and_then(Path::to_str) else {
            return false;
        };
        pkg.entry_wildcards
            .iter()
            .any(|pattern| wildcard_target_matches(pattern, rel))
    }

    /// True when `path` belongs to the published surface of a pre-`exports`-era
    /// library — one whose nearest `package.json` declares a `files` whitelist
    /// but no explicit `main`/`exports`/`module` entry (e.g. express 5.x). Such
    /// a package relies on npm's default `index.js` entry resolution, so its
    /// published surface is the `files` whitelist plus that root `index.js`.
    /// A file inside that surface is reachable only through the package boundary
    /// (an external `require('the-package')`), never `import`ed within the repo,
    /// so the import-graph BFS cannot reach it even though it is genuinely
    /// published — not dead code.
    ///
    /// Scoped to manifests with no explicit entry: once a package declares
    /// `main`/`exports`, [`is_library`] short-circuits the rule and the precise
    /// declared entries ([`is_package_entry_file`]) drive reachability instead,
    /// so this broader `files`-surface heuristic stays inert.
    ///
    /// [`is_library`]: PackageJson::is_library
    /// [`is_package_entry_file`]: ProjectCtx::is_package_entry_file
    pub fn is_in_published_files_surface(&self, path: &Path) -> bool {
        let Some(manifest_dir) = self.nearest_package_json_dir(path) else {
            return false;
        };
        let Some(pkg) = self.nearest_package_json(path) else {
            return false;
        };
        if pkg.is_library || pkg.files.is_empty() {
            return false;
        }
        if manifest_dir.join("index.js") == path || manifest_dir.join("index.ts") == path {
            return true;
        }
        pkg.files.iter().any(|entry| match entry.strip_suffix('/') {
            Some(dir) => path.starts_with(manifest_dir.join(dir)),
            None => manifest_dir.join(entry) == path,
        })
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

    /// True when `path` is the public-API entry file of an Angular library —
    /// either the `lib.entryFile` of the nearest `ng-package.json` (ng-packagr)
    /// or the entry barrel of a Bazel `ng_package` package
    /// ([`is_bazel_ng_package_entry_barrel`]). Both build systems publish the
    /// entry through the build output's `package.json` (`main`/`exports`), not the
    /// source `package.json`, so the source entry and everything it re-exports
    /// look unimported. Rules about "this symbol has no importer" (e.g.
    /// `dead-export`) and "this file is unreachable" (`unused-file`) treat this
    /// file as a package entry point. `path` must be absolute.
    ///
    /// [`is_bazel_ng_package_entry_barrel`]: ProjectCtx::is_bazel_ng_package_entry_barrel
    pub fn is_ng_package_entry_file(&self, path: &Path) -> bool {
        if let Some(manifest_dir) = self.nearest_ng_package_dir(path)
            && let Some(entry_file) = self.ng_package_entry_file(&manifest_dir)
            && manifest_dir.join(entry_file) == path
        {
            return true;
        }
        self.is_bazel_ng_package_entry_barrel(path)
    }

    /// True when `path` lives under the distribution root of a shadcn-style
    /// component registry — a `registry.json` manifest (the
    /// shadcn/shadcn-svelte/shadcn-ui convention) found by walking up from
    /// `path`. Such a manifest lists, under `items[].files[].path`, the source
    /// files the registry CLI (`npx shadcn add …`) fetches and copies into a
    /// consumer's project. Those files are distributed as source artifacts, not
    /// imported as modules within the repo, so their exports have no in-repo
    /// importer yet are part of the registry's published surface. `path` must be
    /// absolute.
    pub fn is_in_distributed_registry_dir(&self, path: &Path) -> bool {
        let Some(start_dir) = path.parent() else {
            return false;
        };
        let root = self.registry_distribution_root(start_dir);
        root.is_some_and(|root| path.starts_with(&root))
    }

    /// Distribution root for `start_dir`: the common-ancestor directory of every
    /// file the nearest enclosing shadcn `registry.json` declares. Resolved once
    /// per starting directory and memoized; `None` when no shadcn registry
    /// manifest encloses `start_dir`.
    fn registry_distribution_root(&self, start_dir: &Path) -> Option<PathBuf> {
        if let Ok(cache) = self.registry_root_cache.lock()
            && let Some(hit) = cache.get(start_dir)
        {
            return hit.clone();
        }
        let manifest_dir =
            walk_up_finding_cached(&self.manifest_dir_cache, start_dir, "registry.json");
        let root = manifest_dir.and_then(|dir| {
            load_manifest_at(&dir, "registry.json", parse_shadcn_registry_file_paths)
                .and_then(|rel_paths| common_ancestor_dir(&dir, &rel_paths))
        });
        if let Ok(mut cache) = self.registry_root_cache.lock() {
            cache
                .entry(start_dir.to_path_buf())
                .or_insert_with(|| root.clone());
        }
        root
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

    /// True when `manifest_dir` contains a `BUILD.bazel` whose contents reference
    /// an `ng_package` rule — Angular's Bazel-built library marker. Read lazily
    /// on first miss and memoized by directory. A bare `BUILD.bazel` without an
    /// `ng_package` target is deliberately NOT a marker: a `BUILD.bazel` can
    /// describe an app or binary just as well as a library.
    fn dir_declares_bazel_ng_package(&self, manifest_dir: &Path) -> bool {
        if let Some(&hit) = self.bazel_ng_package_cache.lock().unwrap().get(manifest_dir) {
            return hit;
        }
        let declares = std::fs::read_to_string(manifest_dir.join("BUILD.bazel"))
            .ok()
            .is_some_and(|raw| build_bazel_declares_ng_package(&raw));
        self.bazel_ng_package_cache
            .lock()
            .unwrap()
            .entry(manifest_dir.to_path_buf())
            .or_insert(declares);
        declares
    }

    /// True when `path` is the public-API entry barrel of a Bazel-built Angular
    /// library: `path` is `index.ts`/`public_api.ts`/`public-api.ts` sitting
    /// directly in a package directory whose `package.json` carries a sibling
    /// `BUILD.bazel` declaring an `ng_package` target. Such a package publishes
    /// its `main`/`exports` from Bazel's build output, not the source
    /// `package.json`, so the barrel and the symbols it re-exports have no in-repo
    /// importer even though they form the package's published surface.
    fn is_bazel_ng_package_entry_barrel(&self, path: &Path) -> bool {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !matches!(stem, "index" | "public_api" | "public-api") {
            return false;
        }
        let Some(dir) = path.parent() else {
            return false;
        };
        let Some(manifest_dir) = self.nearest_package_json_dir(path) else {
            return false;
        };
        // The barrel must sit at the package root, not in a nested subdir.
        dir == manifest_dir && self.dir_declares_bazel_ng_package(&manifest_dir)
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

    /// True when `path` is a module referenced as Vitest's `globalSetup`, whose
    /// `setup`/`teardown` (or default) exports the Vitest runtime calls by name
    /// at run time, never through a static import. Evidence: a `vitest.config.*`
    /// or `vite.config.*` between `path`'s directory and the project root carries
    /// a `globalSetup` option whose referenced path resolves to `path`.
    ///
    /// Gated on the config reference (not the filename) so a `setup` export in an
    /// ordinary module — one no config names as `globalSetup` — stays flaggable.
    fn is_vitest_global_setup_file(&self, path: &Path) -> bool {
        const TEST_CONFIG_FILES: &[&str] = &[
            "vitest.config.ts",
            "vitest.config.js",
            "vitest.config.mts",
            "vitest.config.mjs",
            "vitest.config.cts",
            "vitest.config.cjs",
            "vite.config.ts",
            "vite.config.js",
            "vite.config.mts",
            "vite.config.mjs",
            "vite.config.cts",
            "vite.config.cjs",
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
            for name in TEST_CONFIG_FILES {
                let cfg = d.join(name);
                if !cfg.is_file() {
                    continue;
                }
                if let Ok(raw) = std::fs::read_to_string(&cfg)
                    && config_global_setup_references(&raw, d, path)
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

    /// True when the tsconfig governing `path` declares a
    /// `compilerOptions.jsxImportSource` pointing at a non-React JSX runtime
    /// (anything other than `react`), directly or inherited through its
    /// `extends` chain. Such projects (Qwik, Solid, Preact, …) inject the JSX
    /// factory from that package and use native HTML attribute names in JSX, so
    /// React's camelCase prop conventions do not apply even when a file carries
    /// no framework import. Defaults to false when no tsconfig is found or it
    /// sets no `jsxImportSource`.
    pub fn has_non_react_jsx_import_source(&self, path: &Path) -> bool {
        self.nearest_tsconfig(path)
            .and_then(|tsc| tsc.jsx_import_source.clone())
            .is_some_and(|src| src != "react")
    }

    /// True when the project governing `path` positively declares CommonJS as
    /// its module system, where the resolver supplies extensions and relative
    /// imports therefore need none. The positive signal is the nearest tsconfig
    /// selecting CommonJS output or classic resolution: `compilerOptions.module`
    /// is `commonjs`, or `compilerOptions.module` / `moduleResolution` is one of
    /// `node`/`node10`/`classic`.
    ///
    /// The **nearest config wins**: a `package.json` declaring `"type":"module"`
    /// vetoes the CommonJS signal *only* when no tsconfig opting into classic
    /// resolution sits strictly closer to the file. A subtree (e.g. `tests/`)
    /// with its own tsconfig selecting `moduleResolution:node` is governed by
    /// that closer tsconfig even when a farther-up `package.json` is ESM.
    ///
    /// Conservative by construction: dual-mode signals (`node16`/`nodenext`)
    /// are ESM-capable and want explicit extensions, so they are not CommonJS
    /// here. Absent or ambiguous config returns false — callers keep their
    /// default (ESM) behavior rather than silently assuming CommonJS.
    pub fn is_commonjs_project(&self, path: &Path) -> bool {
        let Some(tsc) = self.nearest_tsconfig(path) else {
            return false;
        };
        const CLASSIC: &[&str] = &["node", "node10", "classic"];
        let module_is_cjs = tsc.module.as_deref().is_some_and(|m| {
            m.eq_ignore_ascii_case("commonjs") || CLASSIC.iter().any(|c| m.eq_ignore_ascii_case(c))
        });
        let resolution_is_classic = tsc
            .module_resolution
            .as_deref()
            .is_some_and(|m| CLASSIC.iter().any(|c| m.eq_ignore_ascii_case(c)));
        if !(module_is_cjs || resolution_is_classic) {
            return false;
        }

        // The tsconfig positively selects CommonJS/classic resolution. An ESM
        // `package.json` (`"type":"module"`) overrides it only when that manifest
        // is at least as close to the file as the tsconfig — i.e. the tsconfig is
        // not in a strictly deeper subtree (the `tests/` case in #1307).
        let pkg_is_esm = self
            .nearest_package_json(path)
            .is_some_and(|pkg| pkg.module_type == ModuleType::Module);
        if !pkg_is_esm {
            return true;
        }
        let (Some(ts_dir), Some(pkg_dir)) = (
            self.nearest_tsconfig_dir(path),
            self.nearest_package_json_dir(path),
        ) else {
            return false;
        };
        // `ts_dir` strictly under `pkg_dir` ⇒ closer tsconfig governs ⇒ CommonJS.
        ts_dir != pkg_dir && ts_dir.starts_with(&pkg_dir)
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

    /// True when the crate owning `path` declares `#![no_std]` at its root.
    ///
    /// The `no_std` attribute is a crate-level inner attribute that lives in the
    /// crate root (`src/lib.rs` / `src/main.rs`), which is usually *not* the same
    /// file as a flagged item — so a per-file `source_contains` check misses it.
    /// This walks up to the nearest `Cargo.toml` (reusing the shared manifest-dir
    /// resolution) and reads that crate's root for a `#![no_std]` /
    /// `#![cfg_attr(..., no_std)]` inner attribute. Cached per crate directory.
    pub fn crate_root_is_no_std(&self, path: &Path) -> bool {
        let Some(start_dir) = path.parent() else {
            return false;
        };
        let Some(crate_dir) =
            walk_up_finding_cached(&self.manifest_dir_cache, start_dir, "Cargo.toml")
        else {
            return false;
        };

        if let Ok(cache) = self.crate_no_std_cache.lock()
            && let Some(hit) = cache.get(&crate_dir)
        {
            return *hit;
        }

        let is_no_std = ["src/lib.rs", "src/main.rs"].iter().any(|root| {
            std::fs::read_to_string(crate_dir.join(root))
                .is_ok_and(|src| source_declares_no_std(&src))
        });

        if let Ok(mut cache) = self.crate_no_std_cache.lock() {
            cache.entry(crate_dir).or_insert(is_no_std);
        }
        is_no_std
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
    /// `deletedAt` field in the project's `schema.prisma` file(s). Returns `None`
    /// when no `schema.prisma` exists anywhere under the project root — callers
    /// should fire on all models in that case to preserve backward-compatible
    /// behaviour. A `Some(set)` (possibly empty) means the schema was found and
    /// only models present in `set` should be flagged.
    ///
    /// Prisma schemas conventionally live in a `prisma/` subdirectory (and in a
    /// monorepo, deep under a `packages/*/prisma/` package), so this scans the
    /// project tree downward rather than walking up from the root — an upward
    /// walk never descends into the subdirectory holding the schema.
    pub fn prisma_soft_delete_models(&self) -> Option<&HashSet<String>> {
        self.prisma_soft_delete_models
            .get_or_init(|| {
                // Anchored on the detected project root only — never the process
                // CWD. An unknown root yields `None` (fire on all models), the
                // same outcome as a project with no `schema.prisma`.
                let root = self.project_root.as_deref()?;
                collect_prisma_soft_delete_models(root)
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

    /// Absolute directories where Prisma's client generator emits its output,
    /// as declared by `generator { output = … }` in the nearest `schema.prisma`
    /// above `path`. The generated client lands in these directories at
    /// `prisma generate` time; they are gitignored and absent in a clean
    /// checkout, so imports resolving into them are expected to be unresolved at
    /// lint time. Returns an empty slice when no `schema.prisma` is found above
    /// `path` or none of its generators declare an `output` (the default
    /// `node_modules/.prisma/client`, already covered by the build-output match).
    /// Walks up per importer — monorepos declare a `schema.prisma` per package —
    /// and memoizes by schema directory, shared by every importer beneath it.
    pub fn prisma_client_output_dirs(&self, path: &Path) -> Arc<Vec<PathBuf>> {
        let empty = || Arc::new(Vec::new());
        let Some(start_dir) = path.parent() else {
            return empty();
        };
        let Some(schema_dir) =
            walk_up_finding_cached(&self.manifest_dir_cache, start_dir, "schema.prisma")
        else {
            return empty();
        };

        if let Some(hit) = self
            .prisma_output_dirs_cache
            .lock()
            .ok()
            .and_then(|c| c.get(&schema_dir).cloned())
        {
            return hit;
        }

        let dirs = std::fs::read_to_string(schema_dir.join("schema.prisma"))
            .ok()
            .map(|schema| {
                parse_prisma_generator_outputs(&schema)
                    .into_iter()
                    .map(|out| schema_dir.join(out))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let arc = Arc::new(dirs);
        if let Ok(mut map) = self.prisma_output_dirs_cache.lock() {
            map.entry(schema_dir).or_insert_with(|| Arc::clone(&arc));
        }
        arc
    }
}

/// Scan the project tree downward from `root` for every `schema.prisma` file
/// (excluding `node_modules` and dot-directories) and aggregate the lowercase
/// names of models that declare a `deletedAt` field. Returns `None` when no
/// `schema.prisma` is found anywhere — the soft-delete rule then falls back to
/// firing on all models. A `Some(set)` (possibly empty) means at least one
/// schema was found, so models absent from `set` provably have no soft-delete
/// column and must not be flagged. Bounded by a depth limit so a pathologically
/// deep tree can't blow the stack or stall.
fn collect_prisma_soft_delete_models(root: &Path) -> Option<HashSet<String>> {
    const MAX_DEPTH: u32 = 8;
    let mut models = HashSet::new();
    let mut found_schema = false;
    let mut stack: Vec<(PathBuf, u32)> = vec![(root.to_path_buf(), 0)];

    while let Some((dir, depth)) = stack.pop() {
        if let Ok(schema) = std::fs::read_to_string(dir.join("schema.prisma")) {
            found_schema = true;
            models.extend(parse_prisma_soft_delete_models(&schema));
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

    found_schema.then_some(models)
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

/// Parse a `schema.prisma` text and return the literal `output` paths declared
/// by each `generator { … }` block. Line-based scan mirroring
/// [`parse_prisma_soft_delete_models`] — Prisma's grammar puts each assignment
/// on its own line, so an `output = "./client"` inside a `generator` block is
/// captured by matching the `output` key and extracting the first quoted string.
/// Non-literal values (`output = env("X")`, no quotes) yield nothing.
fn parse_prisma_generator_outputs(schema: &str) -> Vec<String> {
    let mut outputs = Vec::new();
    let mut in_generator = false;
    let mut depth: i32 = 0;

    for line in schema.lines() {
        let trimmed = line.trim();

        if in_generator {
            for c in trimmed.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            if let Some(rest) = trimmed.strip_prefix("output")
                && rest.trim_start().starts_with('=')
                && let Some(path) = first_quoted_literal(rest)
            {
                outputs.push(path);
            }
            if depth <= 0 {
                in_generator = false;
            }
        } else if trimmed.starts_with("generator ") {
            in_generator = true;
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
    outputs
}

/// Extract the first double-quoted string literal from `s`, returning its inner
/// text. `None` when no closing quote follows the opening one.
fn first_quoted_literal(s: &str) -> Option<String> {
    let start = s.find('"')? + 1;
    let end = s[start..].find('"')? + start;
    Some(s[start..end].to_string())
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

/// Parse + cache the manifest `filename` located directly in `manifest_dir`.
/// Cache hit: clone the `Arc` under the lock. Cache miss: read + parse + insert
/// at the manifest directory. The directory is assumed already resolved (no
/// upward walk), so callers that have stat-walked themselves do not redo it.
fn nearest_parsed_at<T>(
    cache: &Mutex<HashMap<PathBuf, Arc<T>>>,
    manifest_dir: &Path,
    filename: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Option<Arc<T>> {
    if let Some(hit) = cache.lock().ok()?.get(manifest_dir) {
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
        map.entry(manifest_dir.to_path_buf())
            .or_insert_with(|| Arc::clone(&arc));
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

/// True when `src` contains a crate-level `#![no_std]` inner attribute,
/// including the conditional `#![cfg_attr(not(test), no_std)]` form. Matches on
/// an inner-attribute line (`#![`) mentioning `no_std`, so an identifier or
/// comment that merely contains the text `no_std` does not trigger it.
fn source_declares_no_std(src: &str) -> bool {
    src.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("#![") && line.contains("no_std")
    })
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

/// True if `candidate` (a module path that may carry an emitted JS-family
/// extension) points at an existing local source file — directly, with a TS/JS
/// extension appended, with a written `.js`/`.jsx`/`.mjs`/`.cjs` extension
/// resolved to its on-disk TS counterpart, or as a directory containing an
/// `index.*` entry.
fn local_source_exists(candidate: &Path) -> bool {
    if candidate.is_file() {
        return true;
    }
    // TypeScript ESM (`"module": "NodeNext"`/`"Bundler"`) requires writing the
    // emitted `.js` extension in specifiers even when the on-disk source is
    // `.ts`/`.tsx`, so `__helpers/e2e/foo.js` resolves to `foo.ts`. Strip a
    // JS-family extension and probe its TS counterparts on the stem.
    if let Some(ext) = candidate.extension().and_then(|e| e.to_str()) {
        for ts_ext in crate::project::import_index::ts_counterpart_exts(ext) {
            if candidate.with_extension(ts_ext).is_file() {
                return true;
            }
        }
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

/// Manifest-relative paths of every file a shadcn-style `registry.json` declares
/// (each `items[].files[].path`), normalized to forward slashes with any leading
/// `./` stripped. Returns `None` unless the manifest carries the shadcn schema
/// marker (`$schema` ending in `schema/registry.json`) AND a non-empty `items`
/// array yielding at least one file path, so a same-named manifest from an
/// unrelated tool (a `registry.json` of npm package metadata, a Terraform
/// registry config) is not mistaken for a shadcn registry.
fn parse_shadcn_registry_file_paths(raw: &str) -> Option<Vec<String>> {
    let json: Value = serde_json::from_str(raw).ok()?;
    let schema = json.get("$schema").and_then(Value::as_str)?;
    if !schema.ends_with("schema/registry.json") {
        return None;
    }
    let items = json.get("items").and_then(Value::as_array)?;
    let paths: Vec<String> = items
        .iter()
        .filter_map(|item| item.get("files").and_then(Value::as_array))
        .flatten()
        .filter_map(|file| file.get("path").and_then(Value::as_str))
        .map(|p| p.strip_prefix("./").unwrap_or(p).replace('\\', "/"))
        .filter(|p| !p.is_empty())
        .collect();
    if paths.is_empty() {
        return None;
    }
    Some(paths)
}

/// Deepest directory under `base` that contains every entry of `rel_paths`
/// (each manifest-relative, forward-slashed). Returns the absolute common
/// ancestor — for shadcn-svelte's `src/lib/registry/{ui,blocks,…}/…` set this is
/// `base/src/lib/registry`; for a flat shadcn registry (`button.tsx`, `card.tsx`)
/// it is `base` itself. `None` when `rel_paths` is empty.
fn common_ancestor_dir(base: &Path, rel_paths: &[String]) -> Option<PathBuf> {
    let mut iter = rel_paths.iter();
    // Seed with the directory segments of the first path (drop the filename).
    let first = iter.next()?;
    let mut prefix: Vec<&str> = parent_segments(first);
    for path in iter {
        let segs = parent_segments(path);
        let common = prefix
            .iter()
            .zip(segs.iter())
            .take_while(|(a, b)| a == b)
            .count();
        prefix.truncate(common);
        if prefix.is_empty() {
            break;
        }
    }
    let mut root = base.to_path_buf();
    for seg in prefix {
        root.push(seg);
    }
    Some(root)
}

/// Directory segments of a forward-slashed relative file path — the path's
/// segments with the trailing filename removed.
fn parent_segments(rel_path: &str) -> Vec<&str> {
    let mut segs: Vec<&str> = rel_path.split('/').filter(|s| !s.is_empty()).collect();
    segs.pop();
    segs
}

/// True when a `BUILD.bazel`'s raw text invokes the `ng_package` rule —
/// `ng_package(...)`. Matched as a call site (the identifier immediately
/// followed by `(`, allowing whitespace) at a word boundary so neither a longer
/// identifier (`ng_package_test`) nor a bare mention in a comment/load string
/// counts. `ng_package` builds and publishes an Angular npm package, so its
/// presence marks the directory as a library source tree.
fn build_bazel_declares_ng_package(raw: &str) -> bool {
    const RULE: &str = "ng_package";
    let bytes = raw.as_bytes();
    let mut search_from = 0;
    while let Some(rel) = raw[search_from..].find(RULE) {
        let start = search_from + rel;
        let end = start + RULE.len();
        let preceded_by_ident = start
            .checked_sub(1)
            .is_some_and(|i| is_bazel_ident_byte(bytes[i]));
        // The call must read `ng_package(` — skip any whitespace between the
        // identifier and the opening paren.
        let followed_by_call = raw[end..]
            .trim_start()
            .starts_with('(');
        if !preceded_by_ident && followed_by_call {
            return true;
        }
        search_from = end;
    }
    false
}

/// True for bytes that may appear inside a Starlark/Bazel identifier — used to
/// reject a longer identifier that merely ends in `ng_package`.
fn is_bazel_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
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
    fn wildcard_target_matches_substring_and_rejects_non_matches() {
        // `*` expands to a non-empty substring (Node subpath-pattern semantics).
        assert!(wildcard_target_matches("src/locales/*", "src/locales/de.ts"));
        // `*` may span path separators.
        assert!(wildcard_target_matches("src/locales/*", "src/locales/nested/de.ts"));
        // A suffix after `*` must be honored.
        assert!(wildcard_target_matches("dist/*.js", "dist/de.js"));
        assert!(!wildcard_target_matches("dist/*.js", "dist/de.ts"));
        // The spanned substring must be non-empty — prefix/suffix cannot overlap.
        assert!(!wildcard_target_matches("src/locales/*", "src/locales/"));
        // A path outside the prefix never matches.
        assert!(!wildcard_target_matches("src/locales/*", "src/internal/de.ts"));
    }

    #[test]
    fn collect_entry_wildcards_gathers_every_condition() {
        // A non-standard condition (`@zod/source`) is the only one pointing at
        // `.ts` source; it must still be gathered. Literal (non-`*`) targets and
        // a bare-specifier value are excluded.
        let json: Value = serde_json::from_str(
            r#"{"exports":{
                "./locales/*":{"@zod/source":"./src/locales/*","import":"./locales/*.js"},
                "./util":"./src/util.ts",
                "./pkg/*":"some-other-package/*"
            }}"#,
        )
        .unwrap();
        let wildcards = collect_entry_wildcards(&json);
        assert!(wildcards.contains("src/locales/*"), "{wildcards:?}");
        assert!(wildcards.contains("locales/*.js"), "{wildcards:?}");
        // `./util` is a literal target, not a wildcard.
        assert!(!wildcards.contains("src/util.ts"), "{wildcards:?}");
        // A bare specifier (no leading `./`) names no file here.
        assert!(!wildcards.iter().any(|w| w.contains("some-other-package")), "{wildcards:?}");
    }

    #[test]
    fn global_setup_reference_matches_single_and_array_specifiers() {
        let dir = Path::new("/proj");
        let target = Path::new("/proj/global-setup.ts");
        // Single string value.
        assert!(config_global_setup_references(
            "export default { test: { globalSetup: './global-setup.ts' } };",
            dir,
            target,
        ));
        // Array value across lines.
        assert!(config_global_setup_references(
            "globalSetup: [\n  './other.ts',\n  './global-setup.ts',\n]",
            dir,
            target,
        ));
        // Extension-less specifier resolves to the `.ts` target.
        assert!(config_global_setup_references(
            "globalSetup: './global-setup'",
            dir,
            target,
        ));
        // Directory specifier resolving to its index file.
        assert!(config_global_setup_references(
            "globalSetup: './setup'",
            dir,
            Path::new("/proj/setup/index.ts"),
        ));
    }

    #[test]
    fn global_setup_reference_rejects_unrelated_paths() {
        let dir = Path::new("/proj");
        let target = Path::new("/proj/global-setup.ts");
        // No `globalSetup` key at all.
        assert!(!config_global_setup_references(
            "export default { test: { setupFiles: './global-setup.ts' } };",
            dir,
            target,
        ));
        // `globalSetup` names a different module.
        assert!(!config_global_setup_references(
            "globalSetup: './other-setup.ts'",
            dir,
            target,
        ));
        // A look-alike key (`globalSetupReady`) is not the option.
        assert!(!config_global_setup_references(
            "globalSetupReady: './global-setup.ts'",
            dir,
            target,
        ));
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
    fn is_in_distributed_registry_dir_true_for_shadcn_svelte_layout() {
        let dir = TempDir::new().unwrap();
        let docs = dir.path().join("docs");
        std::fs::create_dir_all(docs.join("src/lib/registry/ui/sidebar")).unwrap();
        std::fs::write(
            docs.join("registry.json"),
            r#"{
                "$schema": "https://shadcn-svelte.com/schema/registry.json",
                "name": "r",
                "homepage": "https://x",
                "items": [
                    { "name": "sidebar", "type": "registry:ui",
                      "files": [ { "path": "src/lib/registry/ui/sidebar/index.ts", "type": "registry:ui" } ] },
                    { "name": "dialog", "type": "registry:ui",
                      "files": [ { "path": "src/lib/registry/ui/dialog/index.ts", "type": "registry:ui" } ] }
                ]
            }"#,
        )
        .unwrap();
        let ctx = ProjectCtx::empty();
        assert!(ctx.is_in_distributed_registry_dir(
            &docs.join("src/lib/registry/ui/sidebar/index.ts")
        ));
        // A file outside the common-ancestor registry root is not exempt.
        assert!(!ctx.is_in_distributed_registry_dir(&docs.join("src/lib/utils/orphan.ts")));
    }

    #[test]
    fn is_in_distributed_registry_dir_false_without_shadcn_schema_marker() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src/lib/registry/ui")).unwrap();
        std::fs::write(
            dir.path().join("registry.json"),
            r#"{ "name": "unrelated-tool", "modules": ["a"] }"#,
        )
        .unwrap();
        let ctx = ProjectCtx::empty();
        assert!(!ctx.is_in_distributed_registry_dir(
            &dir.path().join("src/lib/registry/ui/index.ts")
        ));
    }

    #[test]
    fn parse_shadcn_registry_file_paths_requires_schema_and_items() {
        let valid = r#"{
            "$schema": "https://ui.shadcn.com/schema/registry.json",
            "name": "r", "homepage": "https://x",
            "items": [ { "files": [ { "path": "./button.tsx" } ] } ]
        }"#;
        assert_eq!(
            parse_shadcn_registry_file_paths(valid),
            Some(vec!["button.tsx".to_string()])
        );
        // No shadcn schema marker → not a shadcn registry.
        assert_eq!(
            parse_shadcn_registry_file_paths(r#"{ "items": [ { "files": [ { "path": "a.ts" } ] } ] }"#),
            None
        );
        // No file paths → None.
        assert_eq!(
            parse_shadcn_registry_file_paths(
                r#"{ "$schema": "https://x/schema/registry.json", "items": [] }"#
            ),
            None
        );
    }

    #[test]
    fn common_ancestor_dir_returns_shared_prefix() {
        let base = Path::new("/repo/docs");
        let paths = vec![
            "src/lib/registry/ui/sidebar/index.ts".to_string(),
            "src/lib/registry/blocks/hero/hero.svelte".to_string(),
        ];
        assert_eq!(
            common_ancestor_dir(base, &paths),
            Some(PathBuf::from("/repo/docs/src/lib/registry"))
        );
        // A flat registry collapses to the manifest directory itself.
        assert_eq!(
            common_ancestor_dir(base, &["button.tsx".to_string(), "card.tsx".to_string()]),
            Some(PathBuf::from("/repo/docs"))
        );
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

    #[test]
    fn build_bazel_declares_ng_package_matches_rule_call() {
        assert!(build_bazel_declares_ng_package(
            "load(\"//tools:ng_package.bzl\", \"ng_package\")\nng_package(\n    name = \"npm_package\",\n)\n"
        ));
        // Whitespace/newline between the identifier and `(` still counts.
        assert!(build_bazel_declares_ng_package("ng_package\n(name = \"x\")"));
    }

    #[test]
    fn build_bazel_declares_ng_package_rejects_non_calls() {
        // A longer identifier ending in `ng_package` is not the rule.
        assert!(!build_bazel_declares_ng_package("my_ng_package(name = \"x\")"));
        // A bare mention without a call (load string, comment) is not enough.
        assert!(!build_bazel_declares_ng_package(
            "# uses ng_package elsewhere\nts_library(name = \"x\")"
        ));
        // An app/binary BUILD.bazel with no ng_package target.
        assert!(!build_bazel_declares_ng_package(
            "ts_project(name = \"app\")\nng_application(name = \"app\")"
        ));
    }

    #[test]
    fn is_ng_package_entry_file_true_for_bazel_barrel_issue_2299() {
        // Angular source package: placeholder package.json with no
        // main/exports/module, plus a sibling BUILD.bazel declaring ng_package.
        // The package-root `index.ts` is its Bazel-built public-API barrel.
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@angular/animations","version":"0.0.0-PLACEHOLDER","dependencies":{"tslib":"^2.3.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("BUILD.bazel"),
            "load(\"//tools:defaults.bzl\", \"ng_package\")\nng_package(\n    name = \"npm_package\",\n)\n",
        )
        .unwrap();
        let ctx = ProjectCtx::empty();
        assert!(ctx.is_ng_package_entry_file(&dir.path().join("index.ts")));
        assert!(ctx.is_ng_package_entry_file(&dir.path().join("public_api.ts")));
        // A non-barrel file in the package is not an entry — only the barrel.
        assert!(!ctx.is_ng_package_entry_file(&dir.path().join("src/animation.ts")));
    }

    #[test]
    fn is_ng_package_entry_file_false_for_app_with_bare_build_bazel_issue_2299() {
        // Negative-space guard: an app package whose BUILD.bazel declares no
        // ng_package target is NOT a library; its index.ts is not a barrel.
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"my-app","dependencies":{"x":"1"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("BUILD.bazel"), "ts_project(name = \"app\")\n").unwrap();
        let ctx = ProjectCtx::empty();
        assert!(!ctx.is_ng_package_entry_file(&dir.path().join("index.ts")));
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

    /// Build a `ProjectCtx` carrying the `svelte` framework def (which is
    /// detected on both the bare `svelte` compiler and a SvelteKit app) plus
    /// the given `package.json` body.
    #[cfg(test)]
    fn svelte_ctx_with_pkg(pkg_body: &str) -> ProjectCtx {
        let mut ctx = ProjectCtx::default();
        if let Some(fw) = crate::frameworks::get_framework("svelte") {
            ctx.detected_frameworks = vec![fw];
        }
        ctx.package_json = Some(Arc::new(PackageJson::parse(pkg_body).unwrap()));
        ctx
    }

    #[test]
    fn svelte_compiler_is_not_http_api_server() {
        // Issue #3275: the bare `svelte` compiler package is a build tool with
        // no HTTP boundary. It triggers the `svelte` framework def but is not a
        // SvelteKit app, so it must not be classified as an HTTP API server.
        let ctx = svelte_ctx_with_pkg(r#"{"name":"svelte","dependencies":{"acorn":"^8"}}"#);
        assert!(!ctx.is_http_api_server());
    }

    #[test]
    fn sveltekit_app_is_http_api_server() {
        // A genuine SvelteKit app (depends on `@sveltejs/kit`) serves HTTP
        // routes and must be classified as an HTTP API server.
        let ctx = svelte_ctx_with_pkg(
            r#"{"name":"app","devDependencies":{"@sveltejs/kit":"^2","svelte":"^5"}}"#,
        );
        assert!(ctx.is_http_api_server());
    }

    #[test]
    fn express_app_is_http_api_server() {
        // Framework-name path stays intact for dedicated HTTP servers.
        let ctx = ProjectCtx::for_test_with_framework("express");
        assert!(ctx.is_http_api_server());
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
    fn marker_only_package_json_is_transparent() {
        // Regression for #1823 — a `{"type":"module"}` marker manifest in an ESM
        // subtree is not a package boundary. Resolution skips it and uses the
        // substantive root, so both dependency lookup and library detection see
        // the real manifest.
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"msw","main":"./lib/index.js","exports":{".":"./lib/index.js"},"devDependencies":{"vitest":"^1"}}"#,
        )
        .unwrap();
        let sub = dir.path().join("test").join("memory");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.path().join("test").join("package.json"), r#"{"type":"module"}"#).unwrap();

        let ctx = ProjectCtx::empty();
        let pkg = ctx.nearest_package_json(&sub.join("vitest.config.ts")).unwrap();
        assert_eq!(pkg.name.as_deref(), Some("msw"), "skips the marker, resolves the root");
        assert!(pkg.is_library, "library detection uses the substantive root");
        assert!(pkg.has_dep_or_engine("vitest"), "dep lookup uses the substantive root");

        // The directory projection agrees with the parsed manifest.
        let resolved_dir = ctx
            .nearest_package_json_dir(&sub.join("vitest.config.ts"))
            .unwrap();
        assert_eq!(resolved_dir, dir.path(), "dir resolves past the marker to the root");
    }

    #[test]
    fn substantive_nearest_package_json_stays_the_boundary() {
        // Negative space for #1823 — a sub-package that declares its own deps is
        // a real boundary and must NOT be skipped, even when an ancestor exists.
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","devDependencies":{"left-pad":"^1"}}"#,
        )
        .unwrap();
        let sub = dir.path().join("packages").join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            sub.join("package.json"),
            r#"{"name":"@root/sub","dependencies":{"lodash":"^4"}}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        let pkg = ctx.nearest_package_json(&sub.join("index.ts")).unwrap();
        assert_eq!(pkg.name.as_deref(), Some("@root/sub"), "substantive sub stays the boundary");
        assert!(pkg.has_dep_or_engine("lodash"));
        assert!(!pkg.has_dep_or_engine("left-pad"), "root deps are not the sub's boundary");
    }

    #[test]
    fn all_marker_ancestors_fall_back_to_nearest() {
        // When every ancestor manifest is marker-only, resolution still yields
        // the nearest one rather than `None` — it never loses the boundary.
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
        let sub = dir.path().join("src");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("package.json"), r#"{"type":"module"}"#).unwrap();

        let ctx = ProjectCtx::empty();
        let resolved = ctx.nearest_package_json_dir(&sub.join("t.ts"));
        assert_eq!(resolved, Some(sub), "falls back to the nearest marker manifest");
    }

    #[test]
    fn is_marker_only_classifies_fields() {
        let marker = PackageJson::parse(r#"{"type":"module"}"#).unwrap();
        assert!(marker.is_marker_only());
        assert!(PackageJson::parse("{}").unwrap().is_marker_only());

        assert!(!PackageJson::parse(r#"{"name":"x"}"#).unwrap().is_marker_only());
        assert!(!PackageJson::parse(r#"{"main":"./i.js"}"#).unwrap().is_marker_only());
        assert!(!PackageJson::parse(r#"{"exports":{}}"#).unwrap().is_marker_only());
        assert!(!PackageJson::parse(r#"{"module":"./i.mjs"}"#).unwrap().is_marker_only());
        assert!(!PackageJson::parse(r#"{"bin":{"x":"./x.js"}}"#).unwrap().is_marker_only());
        assert!(!PackageJson::parse(r#"{"dependencies":{"a":"1"}}"#).unwrap().is_marker_only());
        assert!(!PackageJson::parse(r#"{"devDependencies":{"a":"1"}}"#).unwrap().is_marker_only());
        assert!(!PackageJson::parse(r#"{"workspaces":["packages/*"]}"#).unwrap().is_marker_only());
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
    fn is_package_entry_file_matches_exports_subpath_targets() {
        // A package that publishes its library as a set of subpath exports
        // (no `.` key) — e.g. `@tiptap/pm` exposing `@tiptap/pm/inputrules` and
        // `@tiptap/pm/state` — makes each target file a real entry point.
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@tiptap/pm","exports":{"./inputrules":"./inputrules/index.ts","./state":{"import":"./state/index.ts"}}}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        assert!(ctx.is_package_entry_file(&dir.path().join("inputrules/index.ts")));
        assert!(ctx.is_package_entry_file(&dir.path().join("state/index.ts")));
        assert!(!ctx.is_package_entry_file(&dir.path().join("other/index.ts")));
    }

    #[test]
    fn is_in_published_files_surface_covers_files_whitelist_and_default_index() {
        // express 5.x pattern: a `files` whitelist, no main/exports/module.
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"express","files":["LICENSE","index.js","lib/"]}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        // npm's default `index.js` entry, even though `files` lists it too.
        assert!(ctx.is_in_published_files_surface(&dir.path().join("index.js")));
        // A file inside a published directory subtree.
        assert!(ctx.is_in_published_files_surface(&dir.path().join("lib/router.js")));
        assert!(ctx.is_in_published_files_surface(&dir.path().join("lib/router/route.js")));
        // A file outside the published surface.
        assert!(!ctx.is_in_published_files_surface(&dir.path().join("internal/scratch.js")));
    }

    #[test]
    fn is_in_published_files_surface_inert_when_main_or_exports_present() {
        // A package with an explicit entry is driven by the precise declared
        // entries, not the broad `files`-surface heuristic.
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"pkg","main":"./lib/index.js","files":["lib/"]}"#,
        )
        .unwrap();

        let ctx = ProjectCtx::empty();
        assert!(!ctx.is_in_published_files_surface(&dir.path().join("lib/router.js")));
        assert!(!ctx.is_in_published_files_surface(&dir.path().join("index.js")));
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
        assert!(first.declares_binary(), "[[bin]] table is present");
        assert!(
            !first.declares_library(),
            "no [lib] table and no src/lib.rs on disk => not a library"
        );
        assert!(first.has_async_runtime(), "tokio is declared");
        assert!(first.is_no_std(), "categories lists no-std");
    }

    const SCHEMA_WITH_ENVELOPE: &str = r#"
model Account {
  id        String   @id @default(cuid())
  provider  String
}

model Envelope {
  id        String    @id @default(cuid())
  deletedAt DateTime?
}
"#;

    #[test]
    fn parse_collects_only_models_with_deleted_at() {
        let models = parse_prisma_soft_delete_models(SCHEMA_WITH_ENVELOPE);
        assert!(models.contains("envelope"));
        assert!(!models.contains("account"));
    }

    #[test]
    fn parse_generator_outputs_extracts_literal_output_paths() {
        // Issue #2293: collect the literal `output` of every generator block, so
        // the import-resolution rules can treat imports into a custom Prisma
        // client output dir as expected-to-exist.
        let schema = r#"
generator client {
  provider = "prisma-client-js"
  output   = "./client"
}

generator edge {
  provider = "prisma-client-js"
  output   = "../generated/edge"
}

datasource db {
  provider = "postgresql"
}

model User {
  id Int @id
}
"#;
        let outputs = parse_prisma_generator_outputs(schema);
        assert_eq!(outputs, vec!["./client".to_string(), "../generated/edge".to_string()]);
    }

    #[test]
    fn parse_generator_outputs_ignores_blocks_without_output() {
        // A generator that declares no `output` uses Prisma's default
        // `node_modules/.prisma/client` (covered by the build-output match), and
        // an `output` assignment outside a generator block is not a client output.
        let schema = r#"
generator client {
  provider = "prisma-client-js"
}

model User {
  output Int
}
"#;
        assert!(parse_prisma_generator_outputs(schema).is_empty());
    }

    // Regression for #1281: Prisma schemas live in a `prisma/` subdirectory, not
    // at the project root, so an upward walk never finds them and the rule fired
    // on every model. The discovery must descend the tree.
    #[test]
    fn collect_finds_schema_in_prisma_subdirectory() {
        let dir = TempDir::new().unwrap();
        let schema_dir = dir.path().join("prisma");
        std::fs::create_dir_all(&schema_dir).unwrap();
        std::fs::write(schema_dir.join("schema.prisma"), SCHEMA_WITH_ENVELOPE).unwrap();

        let models = collect_prisma_soft_delete_models(dir.path()).unwrap();
        assert!(models.contains("envelope"));
        assert!(!models.contains("account"));
    }

    #[test]
    fn collect_finds_schema_in_monorepo_package() {
        let dir = TempDir::new().unwrap();
        let schema_dir = dir.path().join("packages").join("prisma");
        std::fs::create_dir_all(&schema_dir).unwrap();
        std::fs::write(schema_dir.join("schema.prisma"), SCHEMA_WITH_ENVELOPE).unwrap();

        let models = collect_prisma_soft_delete_models(dir.path()).unwrap();
        assert!(models.contains("envelope"));
        assert!(!models.contains("account"));
    }

    #[test]
    fn collect_returns_none_when_no_schema_exists() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        assert!(collect_prisma_soft_delete_models(dir.path()).is_none());
    }

    #[test]
    fn collect_skips_node_modules() {
        let dir = TempDir::new().unwrap();
        let vendored = dir.path().join("node_modules").join("@prisma");
        std::fs::create_dir_all(&vendored).unwrap();
        std::fs::write(vendored.join("schema.prisma"), SCHEMA_WITH_ENVELOPE).unwrap();
        assert!(collect_prisma_soft_delete_models(dir.path()).is_none());
    }

    #[test]
    fn collect_returns_empty_set_when_schema_has_no_soft_delete_model() {
        let dir = TempDir::new().unwrap();
        let schema_dir = dir.path().join("prisma");
        std::fs::create_dir_all(&schema_dir).unwrap();
        std::fs::write(
            schema_dir.join("schema.prisma"),
            "model Account {\n  id String @id\n}",
        )
        .unwrap();
        // Found, but no model has a soft-delete column: Some(empty) so the rule
        // skips every model rather than falling back to firing on all.
        let models = collect_prisma_soft_delete_models(dir.path()).unwrap();
        assert!(models.is_empty());
    }

    /// Helper: load a `ProjectCtx` from explicit `(relative-path, contents)`
    /// pairs under a fresh tempdir and return both, keeping the dir alive.
    fn load_with_files(files: &[(&str, &str)]) -> (TempDir, ProjectCtx) {
        use crate::files::Language;
        let dir = TempDir::new().unwrap();
        let mut sources: Vec<SourceFile> = Vec::new();
        for (rel, body) in files {
            let p = dir.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            if let Some(lang) = Language::from_path(&p) {
                sources.push(SourceFile { path: p, language: lang });
            }
        }
        let refs: Vec<&SourceFile> = sources.iter().collect();
        let ctx = ProjectCtx::load(&refs, &Config::default());
        (dir, ctx)
    }

    // Issue #2080: a private test/harness overlay (`private: true`, no
    // `workspaces`) is not a standalone package — dependency resolution unions
    // the surrounding package's declared deps for files under the overlay.
    #[test]
    fn effective_package_jsons_unions_parent_for_private_overlay() {
        let (dir, ctx) = load_with_files(&[
            ("packages/ls/package.json", r#"{"name":"ls","dependencies":{"vscode-languageserver-protocol":"^3"}}"#),
            ("packages/ls/test/package.json", r#"{"name":"ls-tests","private":true,"dependencies":{"svelte":"^5"}}"#),
            ("packages/ls/test/server.ts", "export const x = 1;"),
        ]);
        let importer = dir.path().join("packages/ls/test/server.ts");
        let chain = ctx.effective_package_jsons(&importer);
        assert!(
            chain.iter().any(|p| p.has_dep_or_engine("vscode-languageserver-protocol")),
            "parent dep must be unioned for a private overlay, chain={:?}",
            chain.iter().map(|p| p.name.clone()).collect::<Vec<_>>()
        );
        // The overlay's own dep is still present.
        assert!(chain.iter().any(|p| p.has_dep_or_engine("svelte")));
    }

    // Negative space for #2080: a non-private nested package is a real,
    // standalone package — its files do NOT inherit the parent's deps.
    #[test]
    fn effective_package_jsons_excludes_parent_for_non_private_nested() {
        let (dir, ctx) = load_with_files(&[
            ("packages/ls/package.json", r#"{"name":"ls","dependencies":{"parent-only":"^1"}}"#),
            ("packages/ls/sub/package.json", r#"{"name":"sub","dependencies":{"svelte":"^5"}}"#),
            ("packages/ls/sub/server.ts", "export const x = 1;"),
        ]);
        let importer = dir.path().join("packages/ls/sub/server.ts");
        let chain = ctx.effective_package_jsons(&importer);
        assert!(
            !chain.iter().any(|p| p.has_dep_or_engine("parent-only")),
            "a non-private nested package must not inherit parent deps"
        );
    }

    // Negative space for #2080: a private nested package that ALSO declares
    // `workspaces` is a workspace root, not an overlay — it does not walk up.
    #[test]
    fn effective_package_jsons_excludes_parent_for_private_workspace_root() {
        let (dir, ctx) = load_with_files(&[
            ("package.json", r#"{"name":"outer","dependencies":{"parent-only":"^1"}}"#),
            ("inner/package.json", r#"{"name":"inner","private":true,"workspaces":["pkgs/*"],"dependencies":{"svelte":"^5"}}"#),
            ("inner/server.ts", "export const x = 1;"),
        ]);
        let importer = dir.path().join("inner/server.ts");
        let chain = ctx.effective_package_jsons(&importer);
        assert!(
            !chain.iter().any(|p| p.has_dep_or_engine("parent-only")),
            "a private workspace root must not inherit parent deps"
        );
    }
}
