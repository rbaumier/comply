//! duplicate-export detection — within each npm package, aggregate every
//! `ReExport` and flag symbol names that show up in two or more distinct barrel
//! files of that package.
//!
//! Re-exports are grouped by the smallest published package surface enclosing
//! each barrel (the nearest `ng-package.json` directory, else the nearest
//! `package.json` directory) so that two independent packages re-exporting the
//! same symbol name are never compared — only barrels inside the same package
//! create an ambiguous import path. This covers both separate workspace packages
//! (a shared `LogLevel` enum across distinct `package.json` roots) and ng-packagr
//! secondary entry points (`@scope/lib`, `@scope/lib/common`) that publish
//! parallel public APIs from nested `ng-package.json` directories under one
//! `package.json`.
//!
//! Skips:
//!   - `"default"` re-exports — barrels routinely re-export a default under
//!     different names; treating that as ambiguous would be noise.
//!   - `"*"` star re-exports — they don't carry a specific name to compare.
//!   - Symbols that appear in only one barrel within a package — no duplication.
//!   - Re-export chains — a barrel re-exporting the name from another barrel in
//!     the same group (e.g. `src/index.ts` re-exporting through `src/core/x.ts`)
//!     is aggregating a single canonical path, not adding an ambiguous one. A
//!     group is flagged only when two or more barrels re-export the name from an
//!     origin outside the group.
//!   - Multi-entry barrels — a package that publishes several barrels as
//!     distinct `exports` subpath entry points (e.g. `.` → `index.ts`,
//!     `./dom` → `dom.ts`) may re-export shared symbols across them for
//!     backward compatibility. Every declared entry-point barrel collapses to a
//!     single canonical public surface, so overlap among them is not ambiguity.
//!   - Namespace-wrapped barrels — a barrel that is consumed only through
//!     `export * as X from './that-barrel'` has every one of its names qualified
//!     under the `X.` namespace at the public surface (`X.Bar`), so its short
//!     names never reach importers flat. Two such barrels that happen to share a
//!     short name (`Bar`) under different wrappers (`X.Bar`, `Y.Bar`) are not an
//!     ambiguous flat import path and are excluded from the count.
//!   - Build-output re-exporters — a barrel re-exporting a name from a compiled
//!     `dist/`/`build/` artifact (`export { RouterLink } from '../dist/foo.js'`)
//!     is re-exporting build output to simulate a consumer, not a source module.
//!     This is the bundle-size measurement / consumer-script shape (e.g.
//!     `size-checks/` entry files), structurally distinct from a source barrel,
//!     so it adds no ambiguous source-import path and is excluded from the count.
//!   - Dev/prod entry-point variants — a library ships parallel dev and prod
//!     variants of one module (`index.ts` / `production.ts`, `foo.dev.ts` /
//!     `foo.prod.ts`) that a bundler or `exports` condition picks between; a
//!     consumer reaches exactly one. Two such variants in the same directory
//!     re-exporting the same name *type-only* are not an ambiguous flat path —
//!     types are erased at compile time and emit no runtime JS — so the variant
//!     group collapses to a single effective path. A runtime-value re-export by
//!     any member is left untouched, so genuine value duplicates still flag.
//!   - Interchangeable multi-adapter barrels — when every barrel re-exporting a
//!     name sources it `from` a *different external* package (icon-set facades
//!     like `lucide-react` / `@phosphor-icons/react` / `@hugeicons/...`), the
//!     barrels are deliberate drop-in alternatives selected by import path and
//!     there is no single canonical barrel to pick. The name is suppressed only
//!     when the re-export sources are all distinct bare specifiers; two barrels
//!     sharing a source, or any relative (in-package) source, leave a genuine
//!     ambiguous path and keep flagging.
//!   - JSX automatic-runtime entries — a `jsx-runtime`/`jsx-dev-runtime` barrel
//!     is a special entry point mandated by the JSX automatic-runtime transform
//!     (React/Preact/etc.). The transform imports `jsx`, `jsxs`, and `Fragment`
//!     from it automatically, so the contract *requires* those symbols to live
//!     there even when the library's main barrel re-exports the same names for
//!     direct consumer use. The two paths serve different consumers (bundler
//!     transform vs. explicit import), so a JSX-runtime barrel adds no ambiguous
//!     flat import path and is excluded from the count.
//!   - Gatsby execution-context entries — `gatsby-ssr`/`gatsby-browser` are the
//!     two Gatsby framework entry files (server-side render vs. browser bundle).
//!     Gatsby's build pipeline consumes each independently and requires both to
//!     re-export the same lifecycle hooks (`wrapRootElement`, `wrapPageElement`)
//!     from a shared module; no user code imports either file. The duplication
//!     is mandated by the framework, so a Gatsby lifecycle entry adds no
//!     ambiguous flat import path and is excluded from the count. Gated on Gatsby
//!     being detected for the file, so an ordinary `gatsby-ssr.js` outside a
//!     Gatsby project still counts.
//!
//! Runs once per project, anchored on the lexicographically smallest indexed
//! path so that a single pass emits all diagnostics deterministically. Barrel
//! paths in the message are shown relative to the project root.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::ProjectCtx;
use crate::project::import_index::ExportKind;
use crate::rules::backend::{CheckCtx, TextCheck};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};

const RULE_ID: &str = "duplicate-export";

/// Per-package re-export occurrences keyed by `(package dir, symbol name)`,
/// each holding the barrel file, line, and module specifier the name is
/// re-exported `from` (the specifier discriminates interchangeable adapters).
type ReExportMap = FxHashMap<(Option<PathBuf>, String), Vec<(PathBuf, usize, Option<String>)>>;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let index = ctx.project.import_index();

        let canon = index.canonical(ctx.path);
        let Some(anchor) = ctx.project.anchor_path() else {
            return Vec::new();
        };
        if canon != anchor {
            return Vec::new();
        }

        // (package dir, symbol name) -> list of (barrel file, line) where it is
        // re-exported. Keying on the nearest `package.json` directory keeps
        // independent packages from being compared against each other; barrels
        // outside any package share a `None` key and compare among themselves.
        let mut reexports: ReExportMap = FxHashMap::default();
        // `(barrel, name)` pairs whose re-export pulls the name out of a compiled
        // `dist/`/`build/` artifact. These barrels re-export build output to
        // simulate a consumer (the bundle-size measurement shape), so they add no
        // ambiguous source-import path and are dropped before counting.
        let mut build_output_reexports: FxHashSet<(PathBuf, String)> = FxHashSet::default();
        for (path, exports) in index.iter_exports() {
            for export in exports {
                if !matches!(export.kind, ExportKind::ReExport) {
                    continue;
                }
                if export.name == "default" || export.name == "*" {
                    continue;
                }
                let package_dir = ctx.project.package_boundary_dir(path);
                if export
                    .reexport_source
                    .as_deref()
                    .is_some_and(specifier_targets_build_output)
                {
                    build_output_reexports.insert((path.to_path_buf(), export.name.clone()));
                }
                reexports
                    .entry((package_dir, export.name.clone()))
                    .or_default()
                    .push((path.to_path_buf(), export.line, export.reexport_source.clone()));
            }
        }

        // Barrels reached only through `export * as X from './barrel'` expose
        // their names qualified under the `X.` namespace, never flat — so a short
        // name shared by two such barrels is `X.Bar` vs `Y.Bar`, not an ambiguous
        // flat path. Collect them once so the count below can drop them.
        let indexed: FxHashSet<&Path> = index.indexed_paths().collect();
        let namespace_wrapped = collect_namespace_wrapped_barrels(&indexed);

        // `(barrel, name)` pairs whose re-export is type-only — either the
        // statement form `export type { X } from '…'` or the per-specifier form
        // `export { type X } from '…'`. The import index records re-exports but
        // not their type-only flag, so it is recovered here by scanning each
        // indexed barrel's source. Type-only re-exports are erased at compile
        // time and emit no runtime JS, which gates the dev/prod-variant collapse
        // below.
        let type_only_reexports = collect_type_only_reexports(&indexed);

        // Indexed barrel paths are canonical; canonicalize the project root once
        // so message paths strip cleanly to project-relative form.
        let root = ctx
            .project
            .project_root
            .as_deref()
            .and_then(|r| std::fs::canonicalize(r).ok());

        let mut diagnostics = Vec::new();
        let mut keys: Vec<&(Option<PathBuf>, String)> = reexports.keys().collect();
        keys.sort();
        for key in keys {
            let (_, name) = key;
            let occurrences = &reexports[key];
            // Need at least two *distinct* barrel files re-exporting the name.
            let mut barrels: Vec<&Path> =
                occurrences.iter().map(|(p, _, _)| p.as_path()).collect();
            barrels.sort();
            barrels.dedup();
            if barrels.len() < 2 {
                continue;
            }
            // A barrel that re-exports the name from another barrel in this same
            // group is the aggregating end of a re-export chain (the standard
            // SDK shape: a top-level `src/index.ts` re-exporting through the
            // source module that actually proxies the implementation). It does
            // not add an independent import path — only barrels whose origin
            // lies outside the group do. Flag only when two or more such
            // independent barrels remain.
            let group: FxHashSet<&Path> = barrels.iter().copied().collect();
            let independent: Vec<&Path> = barrels
                .iter()
                .copied()
                .filter(|barrel| match index.reexport_target(barrel, name) {
                    Some(origin) => !group.contains(origin),
                    None => true,
                })
                .collect();
            // A barrel consumed only through `export * as X from './barrel'`
            // qualifies its names under `X.` — its short names never surface
            // flat, so it adds no ambiguous flat path. Drop those before
            // counting.
            let independent: Vec<&Path> = independent
                .into_iter()
                .filter(|barrel| !namespace_wrapped.contains(*barrel))
                .collect();
            // A barrel re-exporting this name from a compiled `dist/`/`build/`
            // artifact is re-exporting build output to simulate a consumer (the
            // bundle-size measurement shape), not adding a source-import path.
            // Drop it before counting so it can never be one of the two barrels
            // that make a name look ambiguous.
            let independent: Vec<&Path> = independent
                .into_iter()
                .filter(|barrel| {
                    !build_output_reexports.contains(&(barrel.to_path_buf(), name.clone()))
                })
                .collect();
            // A `jsx-runtime`/`jsx-dev-runtime` barrel is a JSX automatic-runtime
            // entry point: the transform imports `jsx`, `jsxs`, and `Fragment`
            // from it automatically, so re-exporting those names there is a
            // contract requirement, not an accidental duplicate of the main
            // barrel. Drop it before counting so it can never be one of the two
            // barrels that make a name look ambiguous.
            let independent: Vec<&Path> = independent
                .into_iter()
                .filter(|barrel| !is_jsx_runtime_barrel(barrel))
                .collect();
            // Gatsby's `gatsby-ssr` and `gatsby-browser` are the two
            // execution-context entry files: the build pipeline consumes each
            // independently (server-side render vs. browser bundle), and the
            // framework requires both to re-export the same lifecycle hooks
            // (`wrapRootElement`, `wrapPageElement`) from a shared module. The
            // duplication is mandated, not an accidental ambiguous barrel, so
            // drop these entries before counting. Gated on Gatsby being detected
            // for the file so an ordinary `gatsby-ssr.js` outside a Gatsby
            // project still flags.
            let independent: Vec<&Path> = independent
                .into_iter()
                .filter(|barrel| !is_gatsby_lifecycle_entry(barrel, ctx.project))
                .collect();
            // Libraries ship dev and prod entry-point variants of one module
            // (`index.ts` / `production.ts`, `foo.dev.ts` / `foo.prod.ts`) that a
            // bundler or export condition picks between. A consumer reaches only
            // one variant, so the variants re-exporting the same *type* are not an
            // ambiguous flat path — types are erased at compile time and emit no
            // runtime JS. Collapse a set of dev/prod variants of one base name
            // into a single effective path when every variant re-exports this name
            // type-only. Runtime-value duplicates across variants are a real
            // ambiguity and are left untouched.
            let independent = collapse_devprod_type_variants(
                independent,
                name,
                &type_only_reexports,
            );
            // A package may publish several barrels as distinct `exports`
            // subpath entry points (e.g. `.` → `index.ts`, `./dom` → `dom.ts`)
            // that intentionally re-export shared symbols for backward
            // compatibility. Those barrels form one canonical public surface,
            // not several ambiguous paths — collapse every declared entry-point
            // barrel into a single effective path before counting.
            let (entry_barrels, plain_barrels): (Vec<&Path>, Vec<&Path>) = independent
                .into_iter()
                .partition(|barrel| ctx.project.is_declared_entry_barrel(barrel));
            let effective_paths = plain_barrels.len() + usize::from(!entry_barrels.is_empty());
            if effective_paths < 2 {
                continue;
            }
            // Interchangeable multi-adapter pattern: when every surviving barrel
            // re-exports this name from a *different external* package (icon-set
            // facades like `lucide-react` / `@phosphor-icons/react`), there is no
            // ambiguous canonical barrel to pick — the barrels are deliberate
            // drop-in alternatives selected by import path. Suppress only when the
            // sources are all distinct bare specifiers; two barrels sharing a
            // source, or any relative (in-package) source, leave a genuine
            // ambiguous path and keep flagging.
            let surviving: Vec<&Path> = entry_barrels
                .iter()
                .chain(plain_barrels.iter())
                .copied()
                .collect();
            if is_interchangeable_adapter_group(&surviving, occurrences) {
                continue;
            }
            // Anchor the diagnostic on the first occurrence (sorted by path)
            // for stable output. List every barrel in the message.
            let first = occurrences
                .iter()
                .min_by(|a, b| a.0.cmp(&b.0))
                .map(|(path, line, _)| (path, *line))
                .expect("at least one occurrence");
            let barrel_list = barrels
                .iter()
                .map(|p| format!("`{}`", display_path(p, root.as_deref())))
                .collect::<Vec<_>>()
                .join(", ");
            diagnostics.push(Diagnostic {
                path: first.0.clone().into(),
                line: first.1,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!(
                    "symbol `{}` is re-exported by multiple barrels ({}). \
                     Pick a single canonical barrel to avoid ambiguous import paths.",
                    name, barrel_list
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

/// Whether `barrels` form an interchangeable multi-adapter group for one name:
/// every barrel re-exports the name `from` a *bare* (external package) specifier
/// and all those specifiers are pairwise distinct. That shape is the deliberate
/// drop-in-adapter / facade pattern (icon-set files re-exporting the same icon
/// names each from a different icon library), where no single canonical barrel
/// exists to pick. Returns `false` — keeping the diagnostic — as soon as two
/// barrels share a source or any source is relative (an in-package module, where
/// the duplicate path is a genuine ambiguity), so real duplicates still flag.
///
/// `occurrences` are the `(barrel, line, source)` records for this `(package,
/// name)` key; the source of each surviving barrel is recovered from them.
fn is_interchangeable_adapter_group(
    barrels: &[&Path],
    occurrences: &[(PathBuf, usize, Option<String>)],
) -> bool {
    let mut sources: FxHashSet<&str> = FxHashSet::default();
    for &barrel in barrels {
        let Some(source) = occurrences
            .iter()
            .find(|(path, _, _)| path == barrel)
            .and_then(|(_, _, source)| source.as_deref())
        else {
            return false;
        };
        if !is_bare_specifier(source) {
            return false;
        }
        if !sources.insert(source) {
            return false;
        }
    }
    sources.len() == barrels.len() && barrels.len() >= 2
}

/// Whether `specifier` names an external package (a bare specifier), not a
/// relative (`.`), absolute (`/`), or `node:` builtin path. A bare specifier
/// points outside the package boundary, marking a barrel as a facade over a
/// third-party implementation rather than an in-package re-export.
fn is_bare_specifier(specifier: &str) -> bool {
    !specifier.is_empty()
        && !specifier.starts_with('.')
        && !specifier.starts_with('/')
        && !specifier.starts_with("node:")
}

/// Render `path` relative to `root` for the diagnostic message, falling back to
/// the full path when it lies outside the root. Keeps the comply install's
/// absolute path out of user-facing output.
fn display_path(path: &Path, root: Option<&Path>) -> String {
    root.and_then(|r| path.strip_prefix(r).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

/// `(barrel path, exported name)` pairs whose re-export is type-only across the
/// indexed set. Covers both the statement form `export type { A, B } from '…'`
/// (every name is type-only) and the per-specifier form
/// `export { type A, B } from '…'` (only `A` is type-only). The import index
/// records re-exports without a type-only flag, so it is recovered by scanning
/// each barrel's source. Only re-export statements (`export … from '…'`) are
/// considered — local `export type { X }` without a `from` clause is a binding,
/// not a barrel re-export.
fn collect_type_only_reexports(indexed: &FxHashSet<&Path>) -> FxHashSet<(PathBuf, String)> {
    let mut out = FxHashSet::default();
    for &file in indexed {
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        for name in type_only_reexport_names(&source) {
            out.insert((file.to_path_buf(), name));
        }
    }
    out
}

/// Exported names that a source's `export … from '…'` statements re-export
/// type-only. A statement-level `export type { … } from` marks every listed
/// name; otherwise each `type`-prefixed specifier (`export { type X, Y } from`)
/// marks only that name. Aliased specifiers report the exported (right-hand)
/// name, matching the index's `name`. Lines without a `from` clause are skipped.
fn type_only_reexport_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("export") else {
            continue;
        };
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = rest.trim_start();
        // Only `export … from '…'` re-exports concern barrels; a clause with no
        // `from` is a local binding, not a re-export.
        if !rest.contains(" from ") {
            continue;
        }
        let stmt_type_only = rest
            .strip_prefix("type")
            .is_some_and(|after| after.starts_with(char::is_whitespace) || after.starts_with('{'));
        let Some(open) = rest.find('{') else {
            continue;
        };
        let Some(close_rel) = rest[open..].find('}') else {
            continue;
        };
        let inner = &rest[open + 1..open + close_rel];
        for spec in inner.split(',') {
            let spec = spec.trim();
            if spec.is_empty() {
                continue;
            }
            let (spec_type_only, body) = match spec.strip_prefix("type") {
                Some(after) if after.starts_with(char::is_whitespace) => (true, after.trim_start()),
                _ => (false, spec),
            };
            if !(stmt_type_only || spec_type_only) {
                continue;
            }
            // `local as exported` re-exports under `exported`; the index keys on
            // that exported name.
            let exported = body
                .split_whitespace()
                .skip_while(|tok| *tok != "as")
                .nth(1)
                .unwrap_or_else(|| body.split_whitespace().next().unwrap_or(body));
            if !exported.is_empty() {
                out.push(exported.to_string());
            }
        }
    }
    out
}

/// Canonical paths of barrels that are the target of an
/// `export * as X from './barrel'` namespace re-export somewhere in the indexed
/// set. The index drops the `export * as X` form entirely (it binds a namespace,
/// not a flat name), so the wrapper is recovered by scanning each indexed file's
/// source for the statement and resolving its specifier against `indexed`.
fn collect_namespace_wrapped_barrels(indexed: &FxHashSet<&Path>) -> FxHashSet<PathBuf> {
    let mut wrapped = FxHashSet::default();
    for &file in indexed {
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        for spec in namespace_reexport_specifiers(&source) {
            if let Some(target) = resolve_relative_specifier(file, &spec, indexed) {
                wrapped.insert(target);
            }
        }
    }
    wrapped
}

/// Specifiers of every `export * as <Ident> from '<spec>'` statement in
/// `source`. Only the namespace form (with `as`) is matched — bare
/// `export * from '<spec>'` binds no namespace and is ignored.
fn namespace_reexport_specifiers(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("export") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('*') else {
            continue;
        };
        let rest = rest.trim_start();
        // Require the `as <ns>` namespace binding; a bare `export * from` has no
        // namespace qualifier and re-exports names flat.
        let Some(rest) = rest.strip_prefix("as") else {
            continue;
        };
        // `as` must be its own token (`as ns`), not a prefix of an identifier.
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        if let Some(spec) = specifier_after_from(rest) {
            out.push(spec);
        }
    }
    out
}

/// Extract the quoted module specifier from the `from '<spec>'` clause within
/// `tail` (the text following `export * as <ns>`). Returns `None` when no `from`
/// clause or string literal is present.
fn specifier_after_from(tail: &str) -> Option<String> {
    let from_idx = tail.find(" from ").or_else(|| tail.find("\tfrom\t"))?;
    let after = tail[from_idx + " from ".len()..].trim_start();
    let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let body = &after[quote.len_utf8()..];
    let end = body.find(quote)?;
    Some(body[..end].to_string())
}

/// Resolve a relative `specifier` declared in `importer` to a path present in
/// `indexed`, probing the same extension and `index` fallbacks the import index
/// uses. Indexed paths are canonical, so candidates are normalized lexically
/// before lookup. Returns `None` for bare specifiers or specifiers that resolve
/// outside the indexed set.
fn resolve_relative_specifier(
    importer: &Path,
    specifier: &str,
    indexed: &FxHashSet<&Path>,
) -> Option<PathBuf> {
    if !specifier.starts_with('.') {
        return None;
    }
    let base = importer.parent()?.join(specifier);
    const EXTS: [&str; 7] = ["ts", "tsx", "js", "jsx", "mts", "mjs", "cts"];
    let base_str = base.to_str()?;
    for ext in EXTS {
        let candidate = lexical_normalize(Path::new(&format!("{base_str}.{ext}")));
        if let Some(&hit) = indexed.get(candidate.as_path()) {
            return Some(hit.to_path_buf());
        }
    }
    for ext in EXTS {
        let candidate = lexical_normalize(&base.join(format!("index.{ext}")));
        if let Some(&hit) = indexed.get(candidate.as_path()) {
            return Some(hit.to_path_buf());
        }
    }
    None
}

/// Whether a re-export specifier reaches into a compiled build-output directory
/// (`dist/` or `build/`). Matches by exact path segment so siblings like
/// `dist-utils/` or `./distance` are not caught. Used to recognize barrels that
/// re-export compiled artifacts to simulate a consumer (bundle-size measurement
/// scripts) rather than re-exporting a source module.
fn specifier_targets_build_output(specifier: &str) -> bool {
    const BUILD_OUTPUT_DIRS: [&str; 2] = ["dist", "build"];
    specifier
        .split('/')
        .any(|segment| BUILD_OUTPUT_DIRS.contains(&segment))
}

/// Whether `barrel` is a JSX automatic-runtime entry point — a file whose stem
/// is `jsx-runtime` or `jsx-dev-runtime`. These are special entries mandated by
/// the JSX transform spec: the compiler imports `jsx`, `jsxs`, and `Fragment`
/// from them automatically, so a library re-exports those names there by
/// contract, in parallel with its main barrel. The match is on the file stem so
/// every extension (`.ts`, `.tsx`, `.js`, ...) is covered.
fn is_jsx_runtime_barrel(barrel: &Path) -> bool {
    barrel
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == "jsx-runtime" || stem == "jsx-dev-runtime")
}

/// Whether `barrel` is a Gatsby execution-context lifecycle entry — a
/// `gatsby-ssr` or `gatsby-browser` file in a project where Gatsby is detected.
/// Gatsby's build pipeline consumes these two files independently (server-side
/// render vs. browser bundle) and requires both to re-export the same lifecycle
/// hooks (`wrapRootElement`, `wrapPageElement`) from a shared module, so the
/// shared symbols are not an ambiguous flat import path. The Gatsby-detection
/// gate (via the file's nearest `package.json`) keeps an ordinary file that
/// happens to be named `gatsby-ssr.js` outside a Gatsby project still flagged.
fn is_gatsby_lifecycle_entry(barrel: &Path, project: &ProjectCtx) -> bool {
    let is_lifecycle_stem = barrel
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == "gatsby-ssr" || stem == "gatsby-browser");
    if !is_lifecycle_stem {
        return false;
    }
    project
        .frameworks_for_path(barrel)
        .iter()
        .any(|fw| fw.name == "gatsby")
}

/// Collapse dev/prod entry-point variants that share a type-only re-export of
/// `name` into a single effective barrel.
///
/// A library ships parallel dev and prod variants of one module (`index.ts` /
/// `production.ts`, `foo.dev.ts` / `foo.prod.ts`) that a bundler or export
/// condition selects between; a consumer reaches exactly one. When two such
/// variants in the same directory re-export the same name *type-only* (erased at
/// compile time, no runtime JS), the overlap is not an ambiguous flat path.
/// Among `barrels`, every group of variants of one base name whose re-export of
/// `name` is type-only across the whole group collapses to a single
/// representative (the lexicographically smallest), so the group counts as one
/// import path. Barrels that are not part of such a group pass through unchanged.
/// A runtime-value re-export by any member leaves the group untouched, preserving
/// detection of genuine value duplicates.
fn collapse_devprod_type_variants<'a>(
    barrels: Vec<&'a Path>,
    name: &str,
    type_only_reexports: &FxHashSet<(PathBuf, String)>,
) -> Vec<&'a Path> {
    // Group candidate barrels by (parent dir, base name) — variants of one
    // module live side by side and differ only in their dev/prod marker.
    let mut groups: FxHashMap<(PathBuf, String), Vec<&'a Path>> = FxHashMap::default();
    let mut passthrough: Vec<&'a Path> = Vec::new();
    for barrel in barrels {
        let is_type_only =
            type_only_reexports.contains(&(barrel.to_path_buf(), name.to_string()));
        match (is_type_only, devprod_variant_key(barrel)) {
            (true, Some(key)) => groups.entry(key).or_default().push(barrel),
            _ => passthrough.push(barrel),
        }
    }
    let mut out = passthrough;
    for (_, mut members) in groups {
        // A single barrel under a base name is not a variant pair — it remains a
        // standalone path. Two or more variants collapse to one representative.
        members.sort();
        out.push(members[0]);
    }
    out
}

/// Key identifying a dev/prod entry-point variant: the barrel's parent directory
/// paired with the module base name once its dev/prod marker is stripped.
/// `index.ts` and `production.ts` are the conventional dev/prod pair for one
/// entry point, so both map to the base `index`. `foo.dev.ts` / `foo.prod.ts`
/// (and `.development` / `.production`) map to the base `foo`. Returns `None`
/// when the stem carries no recognized dev/prod marker — an ordinary barrel is
/// never collapsed.
fn devprod_variant_key(barrel: &Path) -> Option<(PathBuf, String)> {
    let parent = barrel.parent()?.to_path_buf();
    let stem = barrel.file_stem().and_then(|s| s.to_str())?;
    let base = devprod_base_name(stem)?;
    Some((parent, base))
}

/// Base module name a dev/prod variant stem reduces to, or `None` when the stem
/// is not a dev/prod variant. `production` and `index` both reduce to `index`
/// (the `index.ts` / `production.ts` entry-point pair). A trailing
/// `.dev`/`.prod`/`.development`/`.production` segment is stripped to its base
/// (`foo.prod` → `foo`).
fn devprod_base_name(stem: &str) -> Option<String> {
    // `index.ts` / `production.ts` are the conventional dev/prod entry-point
    // pair: both name the same logical entry point. Reduce each to `index` so
    // they share a variant key.
    if stem == "index" || stem == "production" || stem == "development" {
        return Some("index".to_string());
    }
    const MARKERS: [&str; 4] = ["dev", "prod", "development", "production"];
    let (base, marker) = stem.rsplit_once('.')?;
    if base.is_empty() || !MARKERS.contains(&marker) {
        return None;
    }
    Some(base.to_string())
}

/// Resolve `.`/`..` components lexically — no filesystem access. Indexed paths
/// are canonical, so a candidate must be normalized to match them.
fn lexical_normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
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
    use std::sync::Arc;
    use tempfile::TempDir;

    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            // Non-source manifests (e.g. `package.json`) are written to disk so
            // package-boundary detection sees them, but not indexed as sources.
            if let Some(lang) = Language::from_path(&p) {
                source_files.push(SourceFile { path: p, language: lang });
            }
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx, lang: crate::files::Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        (dir, diags)
    }

    /// Pick the file the project's anchor rule will land on so the
    /// once-per-project guard fires inside `run_on_project`. The anchor is the
    /// smallest indexed path that declares exports (`min_indexed`), so JSON
    /// manifests (`package.json`) — indexed but exportless — are excluded.
    fn anchor_rel<'a>(files: &'a [(&'a str, &'a str)]) -> &'a str {
        files
            .iter()
            .map(|(rel, _)| *rel)
            .filter(|rel| {
                Language::from_path(Path::new(rel))
                    .is_some_and(Language::is_typescript_family)
            })
            .min()
            .expect("at least one source file")
    }

    #[test]
    fn flags_symbol_reexported_by_two_barrels() {
        let files: Vec<(&str, &str)> = vec![
            ("impl.ts", "export function compute() {}"),
            ("barrel1.ts", "export { compute } from './impl';"),
            ("barrel2.ts", "export { compute } from './impl';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(diags.len(), 1, "compute is re-exported by two barrels");
        assert_eq!(diags[0].rule_id, "duplicate-export");
        assert!(
            diags[0].message.contains("compute"),
            "message should name the duplicated symbol, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn allows_symbol_reexported_by_only_one_barrel() {
        let files: Vec<(&str, &str)> = vec![
            ("impl.ts", "export function compute() {}"),
            ("barrel.ts", "export { compute } from './impl';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(diags.is_empty(), "single barrel must not be flagged");
    }

    #[test]
    fn skips_default_reexports() {
        let files: Vec<(&str, &str)> = vec![
            ("impl.ts", "export default function compute() {}"),
            ("barrel1.ts", "export { default } from './impl';"),
            ("barrel2.ts", "export { default } from './impl';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "default re-exports must not be flagged, got: {:?}",
            diags
        );
    }

    /// #1082: in a monorepo, two independent packages re-exporting a symbol of
    /// the same name (e.g. `LogLevel`) are separate namespaces and must not be
    /// flagged as duplicates.
    #[test]
    fn allows_same_symbol_across_distinct_packages() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"monorepo","private":true}"#),
            ("sdk/pkg-a/package.json", r#"{"name":"@scope/pkg-a"}"#),
            ("sdk/pkg-a/src/log.ts", "export enum LogLevel { Info }"),
            ("sdk/pkg-a/index.ts", "export { LogLevel } from './src/log';"),
            ("sdk/pkg-b/package.json", r#"{"name":"@scope/pkg-b"}"#),
            ("sdk/pkg-b/src/log.ts", "export enum LogLevel { Error }"),
            ("sdk/pkg-b/index.ts", "export { LogLevel } from './src/log';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "same symbol re-exported by distinct packages must not be flagged, got: {:?}",
            diags
        );
    }

    /// #1836: ng-packagr secondary entry points (Ionic). One `@ionic/angular`
    /// `package.json` publishes parallel public packages — `@ionic/angular`,
    /// `@ionic/angular/common`, `@ionic/angular/standalone` — each declared by a
    /// nested `ng-package.json`. They intentionally re-export the same Angular
    /// component names (`IonModal`, ...) as distinct public APIs. The nearest
    /// `package.json` is identical for all three, so grouping must use the nearest
    /// `ng-package.json` directory as the package boundary; the symbols are not
    /// duplicates across separate entry points.
    #[test]
    fn allows_same_symbol_across_ng_package_entry_points() {
        let files: Vec<(&str, &str)> = vec![
            (
                "packages/angular/package.json",
                r#"{"name":"@ionic/angular"}"#,
            ),
            (
                "packages/angular/ng-package.json",
                r#"{"lib":{"entryFile":"src/index.ts"}}"#,
            ),
            (
                "packages/angular/src/overlays/modal.ts",
                "export class IonModal {}",
            ),
            (
                "packages/angular/src/index.ts",
                "export { IonModal } from './overlays/modal';",
            ),
            (
                "packages/angular/common/ng-package.json",
                r#"{"lib":{"entryFile":"src/index.ts"}}"#,
            ),
            (
                "packages/angular/common/src/overlays/modal.ts",
                "export class IonModal {}",
            ),
            (
                "packages/angular/common/src/index.ts",
                "export { IonModal } from './overlays/modal';",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "same symbol re-exported by distinct ng-package entry points must not be flagged, got: {:?}",
            diags
        );
    }

    /// Two barrels *inside the same package* re-exporting one symbol still
    /// create an ambiguous import path within that package — keep flagging.
    #[test]
    fn flags_two_barrels_within_one_package() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"single-pkg"}"#),
            ("src/impl.ts", "export function compute() {}"),
            ("barrel1.ts", "export { compute } from './src/impl';"),
            ("barrel2.ts", "export { compute } from './src/impl';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(diags.len(), 1, "same-package duplicate must flag, got: {:?}", diags);
        assert!(diags[0].message.contains("compute"));
    }

    /// #1382: the standard SDK shape where a top-level barrel re-exports a
    /// symbol *through* the source module that proxies the implementation is a
    /// single canonical chain (`src/index.ts` → `src/core/uploads.ts` →
    /// `src/internal/uploads.ts`), not two independent barrels. Must not flag.
    #[test]
    fn allows_top_level_barrel_reexporting_through_source_module() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"sdk"}"#),
            ("src/internal/uploads.ts", "export type Uploadable = Blob;"),
            (
                "src/core/uploads.ts",
                "export { type Uploadable } from '../internal/uploads';",
            ),
            (
                "src/index.ts",
                "export { type Uploadable } from './core/uploads';",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "barrel re-exporting through a source module is a single chain, got: {:?}",
            diags
        );
    }

    /// A genuine parallel duplicate alongside a re-export chain still flags: the
    /// independent barrel re-exporting straight from the implementation creates
    /// a second import path that the chain does not collapse.
    #[test]
    fn flags_independent_barrel_alongside_chain() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"sdk"}"#),
            ("src/internal/uploads.ts", "export function toFile() {}"),
            (
                "src/core/uploads.ts",
                "export { toFile } from '../internal/uploads';",
            ),
            (
                "src/index.ts",
                "export { toFile } from './core/uploads';",
            ),
            (
                "src/extra.ts",
                "export { toFile } from './internal/uploads';",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(
            diags.len(),
            1,
            "core/uploads and extra both re-export straight from internal — flag, got: {:?}",
            diags
        );
        assert!(diags[0].message.contains("toFile"));
    }

    /// #1848: a package that publishes two barrels as distinct `exports`
    /// subpath entry points (`.` → `index.ts`, `./dom` → `dom.ts`) may
    /// re-export the same symbols across them for backward compatibility. The
    /// exports targets point at built `dist/` artifacts; matching by stem ties
    /// the source barrels to those entries. Overlap between entry points is
    /// intentional and must not be flagged.
    #[test]
    fn allows_symbol_shared_by_distinct_entry_point_barrels() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"framer-motion","exports":{".":{"import":"./dist/es/index.mjs"},"./dom":{"import":"./dist/es/dom.mjs"}}}"#,
            ),
            (
                "src/dom.ts",
                "export { delayInSeconds as delay, type DelayedFunction } from \"motion-dom\";",
            ),
            (
                "src/index.ts",
                "export { delay, type DelayedFunction } from \"motion-dom\";",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "symbol shared by two declared entry-point barrels is intentional BC, got: {:?}",
            diags
        );
    }

    /// A rogue non-entry barrel re-exporting a symbol the entry points already
    /// share adds a genuine ambiguous import path beyond the public surface, so
    /// it still flags even though two of the three barrels are entry points.
    #[test]
    fn flags_non_entry_barrel_duplicating_entry_point_symbol() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{"name":"framer-motion","exports":{".":{"import":"./dist/es/index.mjs"},"./dom":{"import":"./dist/es/dom.mjs"}}}"#,
            ),
            ("src/dom.ts", "export { delay } from \"motion-dom\";"),
            ("src/index.ts", "export { delay } from \"motion-dom\";"),
            ("src/internal.ts", "export { delay } from \"motion-dom\";"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(
            diags.len(),
            1,
            "rogue non-entry barrel adds a second ambiguous path — flag, got: {:?}",
            diags
        );
        assert!(diags[0].message.contains("delay"));
    }

    /// #1782: two namespace files re-export the same short names (`Bar`,
    /// `Content`, `Label`) from their respective implementation modules, and each
    /// is consumed only through `export * as X from './namespace'`. At the public
    /// surface the names are qualified (`BarList.Bar` vs `BarSegment.Bar`), so the
    /// short-name overlap is not an ambiguous flat import path and must not flag.
    #[test]
    fn allows_same_short_name_in_separate_namespace_objects() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"@chakra-ui/charts"}"#),
            (
                "src/bar-list/bar-list.ts",
                "export function BarListBar() {}\n\
                 export function BarListContent() {}\n\
                 export function BarListLabel() {}",
            ),
            (
                "src/bar-list/namespace.ts",
                "export { BarListBar as Bar, BarListContent as Content, BarListLabel as Label } from \"./bar-list\";",
            ),
            (
                "src/bar-list/index.ts",
                "export { BarListBar, BarListContent, BarListLabel } from \"./bar-list\";\n\
                 export * as BarList from \"./namespace\";",
            ),
            (
                "src/bar-segment/bar-segment.tsx",
                "export function BarSegmentBar() {}\n\
                 export function BarSegmentContent() {}\n\
                 export function BarSegmentLabel() {}",
            ),
            (
                "src/bar-segment/namespace.tsx",
                "export { BarSegmentBar as Bar, BarSegmentContent as Content, BarSegmentLabel as Label } from \"./bar-segment\";",
            ),
            (
                "src/bar-segment/index.ts",
                "export { BarSegmentBar, BarSegmentContent, BarSegmentLabel } from \"./bar-segment\";\n\
                 export * as BarSegment from \"./namespace\";",
            ),
            (
                "src/index.ts",
                "export * from \"./bar-list\";\nexport * from \"./bar-segment\";",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "short names inside separate namespace-wrapped barrels are qualified at the public surface, got: {:?}",
            diags
        );
    }

    /// A namespace wrapper exempts only the barrel it wraps. A third barrel that
    /// re-exports the same short name flat (no `export * as` wrapper) alongside a
    /// namespace-wrapped one still creates a genuine flat path — keep flagging
    /// when two flat barrels remain.
    #[test]
    fn flags_flat_duplicate_alongside_namespace_wrapped_barrel() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"pkg"}"#),
            ("src/impl.ts", "export function Bar() {}"),
            (
                "src/ns/namespace.ts",
                "export { Bar } from \"../impl\";",
            ),
            ("src/ns/index.ts", "export * as Ns from \"./namespace\";"),
            ("src/flat-a.ts", "export { Bar } from \"./impl\";"),
            ("src/flat-b.ts", "export { Bar } from \"./impl\";"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(
            diags.len(),
            1,
            "two flat barrels re-exporting `Bar` remain ambiguous, got: {:?}",
            diags
        );
        assert!(diags[0].message.contains("Bar"));
    }

    /// #1082: barrel paths in the message are relative to the project root —
    /// the comply install's absolute path never leaks.
    #[test]
    fn message_uses_paths_relative_to_project_root() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"single-pkg"}"#),
            ("src/impl.ts", "export function compute() {}"),
            ("barrel1.ts", "export { compute } from './src/impl';"),
            ("barrel2.ts", "export { compute } from './src/impl';"),
        ];
        let target = anchor_rel(&files);
        let (dir, diags) = run_on_project(&files, target);
        assert_eq!(diags.len(), 1);
        let msg = &diags[0].message;
        assert!(
            !msg.contains(&dir.path().display().to_string()),
            "message must not contain the absolute project path, got: {}",
            msg
        );
        assert!(
            msg.contains("`barrel1.ts`") && msg.contains("`barrel2.ts`"),
            "message should list package-relative barrel paths, got: {}",
            msg
        );
    }

    /// #1715: a `size-checks/` entry file re-exports public symbols from the
    /// compiled `dist/` output for bundle-size measurement in CI. It shares those
    /// names with the source barrel `src/index.ts`, but it re-exports build
    /// output to simulate a consumer — not a source module — so it is not an
    /// ambiguous source-import path and must not be flagged.
    #[test]
    fn allows_size_check_barrel_reexporting_from_dist() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"vue-router"}"#),
            (
                "src/RouterLink.ts",
                "export function RouterLink() {}\nexport function RouterView() {}",
            ),
            (
                "src/index.ts",
                "export { RouterLink, RouterView } from './RouterLink';",
            ),
            (
                "size-checks/webRouter_experimental.js",
                "export { RouterLink, RouterView } from '../dist/vue-router.js';",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "a size-check barrel re-exporting from dist/ is a consumer simulation, not an ambiguous barrel, got: {:?}",
            diags
        );
    }

    /// Negative space: two source barrels re-exporting the same name straight
    /// from a source module remain a genuine ambiguous import path. Dropping
    /// `dist/`-sourced re-exporters must not silence real duplicates.
    #[test]
    fn flags_two_source_barrels_despite_dist_exemption() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"vue-router"}"#),
            ("src/RouterLink.ts", "export function RouterLink() {}"),
            ("src/index.ts", "export { RouterLink } from './RouterLink';"),
            ("src/legacy.ts", "export { RouterLink } from './RouterLink';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(
            diags.len(),
            1,
            "two source barrels re-exporting `RouterLink` remain ambiguous, got: {:?}",
            diags
        );
        assert!(diags[0].message.contains("RouterLink"));
    }

    /// #1693: `jsx-runtime.ts` is a JSX automatic-runtime entry point. The JSX
    /// transform imports `Fragment`, `jsx`, and `jsxs` from it automatically, so
    /// the contract requires those symbols to be re-exported there even when the
    /// library's main barrel re-exports the same names for direct consumer use.
    /// Overlap between `jsx-runtime.ts` and `index.ts` is mandated by the spec,
    /// not an accidental duplicate, and must not be flagged.
    #[test]
    fn allows_symbol_shared_by_jsx_runtime_and_main_barrel() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"@scope/ui"}"#),
            (
                "src/runtime/component.ts",
                "export function Fragment() {}\nexport function Frame() {}",
            ),
            (
                "src/runtime/jsx.ts",
                "export function jsx() {}\nexport function jsxs() {}",
            ),
            (
                "src/jsx-runtime.ts",
                "export { Fragment } from './runtime/component';\n\
                 export { jsx, jsxs } from './runtime/jsx';",
            ),
            (
                "src/index.ts",
                "export { Fragment, Frame } from './runtime/component';",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "Fragment shared by jsx-runtime.ts and the main barrel is a JSX contract requirement, got: {:?}",
            diags
        );
    }

    /// `jsx-dev-runtime.ts` is the development-mode counterpart of the JSX
    /// automatic-runtime entry and carries the same exemption.
    #[test]
    fn allows_symbol_shared_by_jsx_dev_runtime_and_main_barrel() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"@scope/ui"}"#),
            (
                "src/runtime/component.ts",
                "export function Fragment() {}",
            ),
            (
                "src/jsx-dev-runtime.ts",
                "export { Fragment } from './runtime/component';",
            ),
            (
                "src/index.ts",
                "export { Fragment } from './runtime/component';",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "Fragment shared by jsx-dev-runtime.ts and the main barrel is a JSX contract requirement, got: {:?}",
            diags
        );
    }

    /// #1711: a library ships a dev/prod split via `index.ts` / `production.ts`
    /// entry-point variants. The `./production` variant strips development tooling
    /// but re-exports the same TypeScript *type* (`TableDevtoolsPreactInit`) as the
    /// `.` entry. Types are erased at compile time and a consumer reaches exactly
    /// one variant, so the shared type is not an ambiguous import path and must not
    /// be flagged — even when the variant origins differ.
    #[test]
    fn allows_type_shared_by_index_and_production_variants() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"preact-table-devtools"}"#),
            (
                "src/PreactTableDevtools.ts",
                "export interface TableDevtoolsPreactInit {}\nexport function TableDevtoolsPanel() {}",
            ),
            (
                "src/production/PreactTableDevtools.ts",
                "export interface TableDevtoolsPreactInit {}\nexport function TableDevtoolsPanel() {}",
            ),
            (
                "src/index.ts",
                "export type { TableDevtoolsPreactInit } from './PreactTableDevtools';",
            ),
            (
                "src/production.ts",
                "export { TableDevtoolsPanel } from './production/PreactTableDevtools';\n\
                 export type { TableDevtoolsPreactInit } from './production/PreactTableDevtools';",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "a type shared by dev/prod entry-point variants must not be flagged, got: {:?}",
            diags
        );
    }

    /// #1711: the same dev/prod split expressed as `*.dev.ts` / `*.prod.ts`
    /// variants of one base name, sharing a type-only re-export, is also exempt.
    #[test]
    fn allows_type_shared_by_dev_prod_suffix_variants() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"@scope/widget"}"#),
            ("src/Widget.ts", "export interface WidgetInit {}"),
            ("src/widget.dev.ts", "export type { WidgetInit } from './Widget';"),
            ("src/widget.prod.ts", "export type { WidgetInit } from './Widget';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "a type shared by *.dev / *.prod variants must not be flagged, got: {:?}",
            diags
        );
    }

    /// Negative space: dev/prod variants sharing a *runtime value* (not a type)
    /// re-export it from the same origin, so a consumer importing the value sees
    /// two paths to it — a genuine ambiguity. The type-only collapse must not
    /// silence value duplicates between variants.
    #[test]
    fn flags_runtime_value_shared_by_dev_prod_variants() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"@scope/widget"}"#),
            ("src/Widget.ts", "export function mountWidget() {}"),
            ("src/widget.dev.ts", "export { mountWidget } from './Widget';"),
            ("src/widget.prod.ts", "export { mountWidget } from './Widget';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(
            diags.len(),
            1,
            "a runtime value shared by dev/prod variants remains ambiguous, got: {:?}",
            diags
        );
        assert!(diags[0].message.contains("mountWidget"));
    }

    /// Negative space: two ordinary source barrels with unrelated names (no
    /// dev/prod marker) re-exporting the same type are still a genuine ambiguous
    /// import path. The dev/prod-variant collapse only fires on recognized
    /// variant names and must not silence real duplicates between plain barrels.
    #[test]
    fn flags_type_shared_by_two_plain_barrels() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"@scope/widget"}"#),
            ("src/Widget.ts", "export interface WidgetInit {}"),
            ("src/entry-a.ts", "export type { WidgetInit } from './Widget';"),
            ("src/entry-b.ts", "export type { WidgetInit } from './Widget';"),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(
            diags.len(),
            1,
            "two plain barrels sharing a type remain ambiguous, got: {:?}",
            diags
        );
        assert!(diags[0].message.contains("WidgetInit"));
    }

    /// Negative space: two ordinary source barrels re-exporting the same name
    /// straight from a source module remain a genuine ambiguous import path. The
    /// JSX-runtime exemption only covers the well-defined `jsx-runtime` /
    /// `jsx-dev-runtime` filenames and must not silence real duplicates between
    /// plain barrels.
    #[test]
    fn flags_two_plain_barrels_despite_jsx_runtime_exemption() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"@scope/ui"}"#),
            (
                "src/runtime/component.ts",
                "export function Fragment() {}",
            ),
            (
                "src/index.ts",
                "export { Fragment } from './runtime/component';",
            ),
            (
                "src/legacy.ts",
                "export { Fragment } from './runtime/component';",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(
            diags.len(),
            1,
            "two ordinary source barrels re-exporting `Fragment` remain ambiguous, got: {:?}",
            diags
        );
        assert!(diags[0].message.contains("Fragment"));
    }

    /// #1632: shadcn-ui/ui's `apps/v4/registry/icons/` ships three interchangeable
    /// icon-library adapter barrels — `__hugeicons__.ts`, `__lucide__.ts`,
    /// `__phosphor__.ts` — that each re-export the same icon names (`ActivityIcon`)
    /// from a *different* third-party package. They are drop-in alternatives an app
    /// switches between by import path, not an ambiguous canonical barrel, so the
    /// shared name must not be flagged.
    #[test]
    fn allows_same_symbol_across_interchangeable_adapter_barrels() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"@scope/ui"}"#),
            (
                "icons/__hugeicons__.ts",
                "export { ActivityIcon } from \"@hugeicons/core-free-icons\";",
            ),
            (
                "icons/__lucide__.ts",
                "export { ActivityIcon } from \"lucide-react\";",
            ),
            (
                "icons/__phosphor__.ts",
                "export { ActivityIcon } from \"@phosphor-icons/react\";",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "icons re-exported from distinct external packages are interchangeable adapters, got: {:?}",
            diags
        );
    }

    /// #2152: a Gatsby site's `gatsby-ssr.js` and `gatsby-browser.js` are two
    /// distinct framework entry files (SSR vs. browser execution context) that
    /// must both re-export the same lifecycle hooks (`wrapRootElement`,
    /// `wrapPageElement`) from a shared module. Gatsby's build pipeline consumes
    /// each file independently; no user code imports from either, so the shared
    /// symbols are not an ambiguous flat import path and must not be flagged.
    #[test]
    fn allows_lifecycle_hooks_shared_by_gatsby_ssr_and_browser_entries() {
        let files: Vec<(&str, &str)> = vec![
            (
                "website/package.json",
                r#"{"name":"website","dependencies":{"gatsby":"5.0.0"}}"#,
            ),
            (
                "website/gatsby-shared.js",
                "export const wrapRootElement = () => {};\n\
                 export const wrapPageElement = () => {};",
            ),
            (
                "website/gatsby-ssr.js",
                "export { wrapRootElement, wrapPageElement } from './gatsby-shared.js';",
            ),
            (
                "website/gatsby-browser.js",
                "export { wrapRootElement, wrapPageElement } from './gatsby-shared.js';",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert!(
            diags.is_empty(),
            "lifecycle hooks shared by gatsby-ssr and gatsby-browser entries are framework-required, got: {:?}",
            diags
        );
    }

    /// Negative space: the Gatsby exemption removes only the `gatsby-ssr` /
    /// `gatsby-browser` entries from the count (they are consumed by Gatsby's
    /// pipeline, never imported by user code). Two *ordinary* barrels sharing the
    /// same symbol alongside a Gatsby entry remain a genuine ambiguous flat import
    /// path and must still flag.
    #[test]
    fn flags_two_plain_barrels_despite_gatsby_exemption() {
        let files: Vec<(&str, &str)> = vec![
            (
                "website/package.json",
                r#"{"name":"website","dependencies":{"gatsby":"5.0.0"}}"#,
            ),
            (
                "website/gatsby-shared.js",
                "export const wrapPageElement = () => {};",
            ),
            (
                "website/gatsby-ssr.js",
                "export { wrapPageElement } from './gatsby-shared.js';",
            ),
            (
                "website/barrel-a.js",
                "export { wrapPageElement } from './gatsby-shared.js';",
            ),
            (
                "website/barrel-b.js",
                "export { wrapPageElement } from './gatsby-shared.js';",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(
            diags.len(),
            1,
            "two ordinary barrels re-exporting `wrapPageElement` remain ambiguous, got: {:?}",
            diags
        );
        assert!(diags[0].message.contains("wrapPageElement"));
    }

    /// Negative space: the exemption is gated on Gatsby being detected. A pair of
    /// barrels named `gatsby-ssr.js` / `gatsby-browser.js` in a project with no
    /// `gatsby` dependency are ordinary files and their shared symbol still flags.
    #[test]
    fn flags_gatsby_named_files_without_gatsby_dependency() {
        let files: Vec<(&str, &str)> = vec![
            ("website/package.json", r#"{"name":"website"}"#),
            (
                "website/gatsby-shared.js",
                "export const wrapPageElement = () => {};",
            ),
            (
                "website/gatsby-ssr.js",
                "export { wrapPageElement } from './gatsby-shared.js';",
            ),
            (
                "website/gatsby-browser.js",
                "export { wrapPageElement } from './gatsby-shared.js';",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(
            diags.len(),
            1,
            "gatsby-named files without the gatsby dependency are ordinary barrels, got: {:?}",
            diags
        );
        assert!(diags[0].message.contains("wrapPageElement"));
    }

    /// Negative space: two barrels in one directory re-exporting the same name
    /// from the *same* source (`./icons`) are a genuine ambiguous import path, not
    /// interchangeable adapters. The distinct-source discriminator must not
    /// silence a real duplicate.
    #[test]
    fn flags_two_barrels_reexporting_from_same_source() {
        let files: Vec<(&str, &str)> = vec![
            ("package.json", r#"{"name":"@scope/ui"}"#),
            ("icons/icons.ts", "export function ActivityIcon() {}"),
            (
                "icons/__a__.ts",
                "export { ActivityIcon } from \"./icons\";",
            ),
            (
                "icons/__b__.ts",
                "export { ActivityIcon } from \"./icons\";",
            ),
        ];
        let target = anchor_rel(&files);
        let (_dir, diags) = run_on_project(&files, target);
        assert_eq!(
            diags.len(),
            1,
            "two barrels re-exporting `ActivityIcon` from the same source remain ambiguous, got: {:?}",
            diags
        );
        assert!(diags[0].message.contains("ActivityIcon"));
    }
}
