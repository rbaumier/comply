//! Thin wrapper around oxc_parser + oxc_semantic for rules that need
//! true scope analysis (cross-scope reference tracking, shadowing,
//! unused symbols) instead of the heuristic tree-sitter walks.
//!
//! `oxc_ast` borrows from a bump `Allocator` for the whole AST lifetime,
//! so we expose a closure-based API instead of returning the `Semantic`:
//! the allocator lives on the stack of `with_semantic` and gets dropped
//! when the closure returns.

use std::cell::RefCell;
use std::path::Path;

use rustc_hash::{FxHashMap, FxHashSet};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::SourceType;

/// Per-file memo backing [`source_contains`]. `ptr`/`len` capture the identity
/// of the source the `hits` map describes; when either changes the map is
/// cleared, so a stale entry from a previous file (or test source) can never be
/// returned. The engine also calls [`reset_file_caches`] once per file for
/// deterministic hot-path invalidation.
#[derive(Default)]
struct SourceContainsCache {
    ptr: usize,
    len: usize,
    hits: FxHashMap<String, bool>,
}

thread_local! {
    static SOURCE_CONTAINS: RefCell<SourceContainsCache> =
        RefCell::new(SourceContainsCache::default());
}

/// Per-file index of line-start byte offsets backing [`byte_offset_to_line_col`].
/// `starts[k]` is the byte offset where line `k + 1` begins (`starts[0] == 0`).
/// Like [`SourceContainsCache`] it is keyed by the source `(ptr, len)` identity
/// and rebuilt when that changes; the engine also clears it once per file.
#[derive(Default)]
struct LineIndex {
    ptr: usize,
    len: usize,
    starts: Vec<usize>,
}

thread_local! {
    static LINE_INDEX: RefCell<LineIndex> = RefCell::new(LineIndex::default());
}

/// Per-file memo slots for expensive file-invariant predicates (e.g. "does the
/// project use React Compiler?", "is this file an ESM context?"). Each slot is
/// keyed by the source `(ptr, len)` identity and cleared once per file by
/// [`reset_file_caches`], so a reused worker buffer can never serve a stale
/// result. Unlike a `HashMap`-keyed memo this is pure integer compares — rules
/// that call it from per-node `OxcCheck::run` pay no hashing on the hot path.
#[derive(Default, Clone, Copy)]
struct FileBoolSlot {
    ptr: usize,
    len: usize,
    val: Option<bool>,
}

/// Slot index for [`cached_file_bool`] — one per distinct file-invariant
/// predicate. Keep these unique; collisions would cross-contaminate caches.
pub const SLOT_REACT_COMPILER: usize = 0;
pub const SLOT_ES_MODULE: usize = 1;
pub const SLOT_TESTING_MSW: usize = 2;
pub const SLOT_PLAYWRIGHT: usize = 3;
pub const SLOT_DELETED_AT_COLUMN: usize = 4;
pub const SLOT_TYPE_ONLY_FILE: usize = 5;
pub const SLOT_WORKER_SCRIPT: usize = 6;
pub const SLOT_PROTOCOL_MANDATED_WEAK_HASH: usize = 7;
const FILE_BOOL_SLOTS: usize = 8;

/// Per-file memo backing [`file_typeof_guards`]: the set of global identifiers
/// (`window`/`self`/`global`) the current file feature-detects with a `typeof`
/// check. Keyed by the source `(ptr, len)` identity and rebuilt when that
/// changes, like the other per-file caches.
#[derive(Default, Clone, Copy)]
struct TypeofGuardSlot {
    ptr: usize,
    len: usize,
    guards: Option<TypeofGuards>,
}

/// Which of the three global aliases a file guards with a `typeof` check.
#[derive(Default, Clone, Copy)]
pub struct TypeofGuards {
    pub window: bool,
    pub self_: bool,
    pub global: bool,
}

impl TypeofGuards {
    /// True if `name` (`window`/`self`/`global`) is `typeof`-guarded in the file.
    #[must_use]
    pub fn guards(&self, name: &str) -> bool {
        match name {
            "window" => self.window,
            "self" => self.self_,
            "global" => self.global,
            _ => false,
        }
    }
}

thread_local! {
    static TYPEOF_GUARDS: std::cell::Cell<TypeofGuardSlot> =
        const { std::cell::Cell::new(TypeofGuardSlot { ptr: 0, len: 0, guards: None }) };
}

thread_local! {
    static FILE_BOOLS: std::cell::Cell<[FileBoolSlot; FILE_BOOL_SLOTS]> = const {
        std::cell::Cell::new([FileBoolSlot { ptr: 0, len: 0, val: None }; FILE_BOOL_SLOTS])
    };
}

/// Memoize a file-invariant boolean predicate for the duration of the current
/// file. `compute` runs at most once per file per `slot`; subsequent per-node
/// calls return the cached value via integer-only `(ptr, len)` comparison.
pub fn cached_file_bool<F: FnOnce() -> bool>(source: &str, slot: usize, compute: F) -> bool {
    let ptr = source.as_ptr() as usize;
    let len = source.len();
    let cur = FILE_BOOLS.with(std::cell::Cell::get)[slot];
    if cur.ptr == ptr && cur.len == len && let Some(v) = cur.val {
        return v;
    }
    let v = compute();
    FILE_BOOLS.with(|c| {
        let mut arr = c.get();
        arr[slot] = FileBoolSlot {
            ptr,
            len,
            val: Some(v),
        };
        c.set(arr);
    });
    v
}

/// Clear every per-file memo (`source_contains` hits and the line-start index).
/// Called once per file by the engine before any backend runs, so a reused
/// worker source buffer that happens to share a `(ptr, len)` with the previous
/// file can never serve a stale entry.
pub fn reset_file_caches() {
    SOURCE_CONTAINS.with(|c| {
        let mut c = c.borrow_mut();
        c.ptr = 0;
        c.len = 0;
        c.hits.clear();
    });
    LINE_INDEX.with(|c| {
        let mut c = c.borrow_mut();
        c.ptr = 0;
        c.len = 0;
        c.starts.clear();
    });
    FILE_BOOLS.with(|c| c.set([FileBoolSlot::default(); FILE_BOOL_SLOTS]));
    TYPEOF_GUARDS.with(|c| c.set(TypeofGuardSlot::default()));
}

/// Memoized `source.contains(needle)` for the current file. `source.contains`
/// is O(file-size); rules call this from per-node `OxcCheck::run`, so without
/// the memo a file of N nodes costs O(N × file-size). The result is constant
/// for a given source, so we scan once per distinct needle. The cache
/// auto-invalidates when `source`'s `(ptr, len)` identity changes.
pub fn source_contains(source: &str, needle: &str) -> bool {
    let ptr = source.as_ptr() as usize;
    let len = source.len();
    SOURCE_CONTAINS.with(|c| {
        let mut c = c.borrow_mut();
        if c.ptr != ptr || c.len != len {
            c.ptr = ptr;
            c.len = len;
            c.hits.clear();
        }
        if let Some(&hit) = c.hits.get(needle) {
            return hit;
        }
        let hit = memchr::memmem::find(source.as_bytes(), needle.as_bytes()).is_some();
        c.hits.insert(needle.to_string(), hit);
        hit
    })
}

/// Collect every property key destructured from a `<expr>.groups` object
/// pattern anywhere in `source`, e.g. `code`/`openingFence` from
/// `const {code, openingFence} = match.groups ?? {}`.
///
/// A regex's named capturing groups are consumed not only through direct
/// property access (`match.groups.name`) but also through object
/// destructuring of the `.groups` object. This returns the destructured
/// keys so callers can treat them as references.
///
/// Conservative (zero false negatives): keys from any `.groups` destructure
/// are returned without tying them back to a specific regex, since the
/// source expression of the `.groups` access isn't resolved to its
/// originating pattern. For a renamed binding (`{ year: y }`) the KEY
/// (`year`, the group name) is collected, not the local binding.
#[must_use]
pub fn groups_destructure_keys(source: &str) -> FxHashSet<String> {
    let mut keys = FxHashSet::default();
    let bytes = source.as_bytes();
    let needle = b".groups";
    let mut search_from = 0;
    while let Some(rel) = memchr::memmem::find(&bytes[search_from..], needle) {
        let dot = search_from + rel;
        let after = dot + needle.len();
        search_from = after;
        // Require `.groups` to be a member access, not a prefix of a longer
        // identifier such as `.groupsCount`.
        if bytes.get(after).is_some_and(|&b| is_ident_byte(b)) {
            continue;
        }
        // The destructuring shape is `{ ... } = <expr>.groups`, so the
        // object pattern sits to the LEFT of the `<expr>.groups` access.
        let Some(brace_close) = object_pattern_before_groups(bytes, dot) else {
            continue;
        };
        let Some(brace_open) = matching_open_brace(bytes, brace_close) else {
            continue;
        };
        collect_destructured_keys(&source[brace_open + 1..brace_close], &mut keys);
    }
    keys
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Given the byte offset of the `.` in a `.groups` access, find the closing
/// `}` of the object pattern that destructures it, i.e. the `}` in
/// `{ ... } = <expr>.groups`. Returns `None` if the access isn't the RHS of
/// a destructuring assignment.
fn object_pattern_before_groups(bytes: &[u8], groups_dot: usize) -> Option<usize> {
    let mut i = groups_dot;
    // Walk left over the source expression `<expr>` (`match`, `re.exec(s)?`,
    // …) back to the top-level (`depth == 0`) `=` that assigns it.
    let mut depth: i32 = 0;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' if depth > 0 => depth -= 1,
            b'{' | b';' if depth == 0 => return None,
            b'=' if depth == 0 => {
                // Reject `==`, `===`, `<=`, `>=`, `!=`, `=>`.
                let prev = bytes[..i].iter().rev().find(|&&b| !b.is_ascii_whitespace());
                if matches!(prev, Some(b'=' | b'!' | b'<' | b'>')) {
                    return None;
                }
                if bytes.get(i + 1) == Some(&b'>') {
                    return None;
                }
                // To the left of `=`, skipping whitespace, must be `}`.
                let mut j = i;
                while j > 0 {
                    j -= 1;
                    if bytes[j].is_ascii_whitespace() {
                        continue;
                    }
                    return (bytes[j] == b'}').then_some(j);
                }
                return None;
            }
            _ => {}
        }
    }
    None
}

/// Find the `{` matching the closing `}` at `brace_close`, scanning left and
/// honouring nested braces.
fn matching_open_brace(bytes: &[u8], brace_close: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut i = brace_close;
    loop {
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}

/// Parse the destructured property keys out of an object-pattern body (the
/// text between `{` and `}`). For shorthand `code` the key is `code`; for
/// renamed `code: c` the key is `code`. Skips rest (`...x`) and computed
/// (`[expr]`) properties, and ignores default values (`= …`).
fn collect_destructured_keys(pattern_body: &str, keys: &mut FxHashSet<String>) {
    for segment in split_top_level_commas(pattern_body) {
        let segment = segment.trim();
        if segment.is_empty() || segment.starts_with("...") || segment.starts_with('[') {
            continue;
        }
        // The key is the identifier before `:` (renamed) or `=` (default),
        // whichever comes first.
        let key_part = segment.split([':', '=']).next().unwrap_or(segment).trim();
        let key: String = key_part
            .chars()
            .take_while(|&c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            .collect();
        if !key.is_empty() {
            keys.insert(key);
        }
    }
}

/// Split an object-pattern body on commas that sit at bracket depth 0, so a
/// nested pattern (`{ a: { b } }`) or default object (`= {}`) doesn't split
/// mid-property.
fn split_top_level_commas(body: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, b) in body.bytes().enumerate() {
        match b {
            b'{' | b'[' | b'(' => depth += 1,
            b'}' | b']' | b')' => depth -= 1,
            b',' if depth == 0 => {
                segments.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    segments.push(&body[start..]);
    segments
}

/// HTTP/WebSocket protocol fields whose digest algorithm is fixed by an RFC.
/// MD5/SHA-1 here is a protocol contract, not a security choice: the peer
/// decodes the field assuming that exact algorithm, so "use SHA-256" would
/// break interop. Matched case-insensitively (header names are case-insensitive
/// per spec).
const PROTOCOL_MANDATED_HASH_FIELDS: &[&str] = &[
    "content-md5", // RFC 1864 — Content-MD5 entity header / trailer (MD5)
    "sec-websocket-key", // RFC 6455 — WebSocket handshake key (SHA-1)
    "sec-websocket-accept", // RFC 6455 — WebSocket handshake accept (SHA-1)
];

/// True when the file references a protocol field whose hash algorithm is
/// mandated by an RFC (see [`PROTOCOL_MANDATED_HASH_FIELDS`]). In such files an
/// MD5/SHA-1 digest is computed to satisfy the protocol, not chosen for
/// collision resistance, so `no-weak-hashing` must not fire. The signal is the
/// field name, not the surrounding variable names, so it survives renaming and
/// generalizes across every site that produces the same protocol field.
///
/// Memoized per file via [`source_contains`]; the match is case-insensitive, so
/// the lowercased source is scanned for the (already lowercase) field names.
#[must_use]
pub fn references_protocol_mandated_weak_hash(source: &str) -> bool {
    // Field names are short and rare; scanning the lowercased source once and
    // memoizing the result keeps this off the per-node hot path.
    cached_file_bool(source, SLOT_PROTOCOL_MANDATED_WEAK_HASH, || {
        let lower = source.to_ascii_lowercase();
        PROTOCOL_MANDATED_HASH_FIELDS
            .iter()
            .any(|field| memchr::memmem::find(lower.as_bytes(), field.as_bytes()).is_some())
    })
}

/// True if the file imports anything from `react`, `react-dom`, or a `react/*`
/// subpath (ESM `import ... from` or CommonJS `require(...)`). React-specific
/// rules (render-reference equality, hook semantics) use this to skip files that
/// use JSX with a non-React framework (remix/ui, SolidJS, Preact, Vue JSX).
/// Memoized per file via [`source_contains`].
#[must_use]
pub fn imports_react(source: &str) -> bool {
    source_contains(source, "from \"react\"")
        || source_contains(source, "from 'react'")
        || source_contains(source, "from \"react-dom")
        || source_contains(source, "from 'react-dom")
        || source_contains(source, "from \"react/")
        || source_contains(source, "from 'react/")
        || source_contains(source, "require(\"react\")")
        || source_contains(source, "require('react')")
        || source_contains(source, "require(\"react-dom")
        || source_contains(source, "require('react-dom")
}

/// True if the file imports anything from SolidJS: `solid-js`, a `solid-js/*`
/// subpath (`solid-js/web`, `solid-js/store`), or the `@solidjs/*` scope
/// (`@solidjs/router`, `@solidjs/start`) — ESM `import ... from` or CommonJS
/// `require(...)`. React-specific rules use this to exclude SolidJS files, whose
/// fine-grained reactivity has no component re-render cycle (the body runs once),
/// so React-render concerns do not apply. Memoized per file via [`source_contains`].
#[must_use]
pub fn imports_solid(source: &str) -> bool {
    source_contains(source, "from \"solid-js\"")
        || source_contains(source, "from 'solid-js'")
        || source_contains(source, "from \"solid-js/")
        || source_contains(source, "from 'solid-js/")
        || source_contains(source, "from \"@solidjs/")
        || source_contains(source, "from '@solidjs/")
        || source_contains(source, "require(\"solid-js")
        || source_contains(source, "require('solid-js")
        || source_contains(source, "require(\"@solidjs/")
        || source_contains(source, "require('@solidjs/")
}

/// True when `path`'s nearest `package.json` declares a non-React JSX framework
/// (`vue` or `solid-js`) and does **not** declare `react`. React-only render-churn
/// rules (`react-jsx-no-bind`, `jsx-no-new-function-as-prop`) use this to skip
/// files that belong to a Vue or Solid package: those frameworks have their own
/// reactivity (Vue) or fine-grained reactivity (Solid), so a fresh inline
/// function reference per render is not a re-render hazard there.
///
/// Resolution is per-file via [`ProjectCtx::nearest_package_json`], so in a
/// monorepo a `vue`-declaring `examples/vue/package.json` is detected even when
/// the repo root declares `react`. A file that declares both `react` and a
/// non-React framework keeps firing (ambiguous — default to the React intent); a
/// file whose nearest manifest resolves nothing keeps firing (default-on).
///
/// [`ProjectCtx::nearest_package_json`]: crate::project::ProjectCtx::nearest_package_json
#[must_use]
pub fn in_non_react_framework_package(
    project: &crate::project::ProjectCtx,
    path: &Path,
) -> bool {
    let Some(pkg) = project.nearest_package_json(path) else {
        return false;
    };
    let declares = |name: &str| {
        pkg.dependencies.contains_key(name)
            || pkg.dev_dependencies.contains_key(name)
            || pkg.peer_dependencies.contains_key(name)
            || pkg.optional_dependencies.contains_key(name)
    };
    (declares("vue") || declares("solid-js")) && !declares("react")
}

/// True when the file is JSX for a framework that uses native HTML attribute
/// names (`class`, `for`, …) rather than React's camelCase — Vue, Solid,
/// Preact, Qwik, or Stencil. Detected three ways: via a framework import, via an
/// in-file `@jsxImportSource` pragma, or via the nearest `tsconfig.json`'s
/// `compilerOptions.jsxImportSource` set to a non-React runtime (which injects
/// the JSX factory project-wide, so files need no framework import).
///
/// React-specific rules (`no-unknown-property`, `react-display-name`) must not
/// fire on these files: React DevTools, Fast Refresh, and React's prop
/// conventions are all React-only concerns. Source checks are memoized per file
/// via [`source_contains`].
#[must_use]
pub fn is_non_react_jsx_file(source: &str, project: &crate::project::ProjectCtx, path: &Path) -> bool {
    source_contains(source, "solid-js")
        || source_contains(source, "@solidjs/")
        || source_contains(source, "solid-start")
        || source_contains(source, "@tanstack/solid-router")
        || source_contains(source, "@vue/")
        || source_contains(source, "@builder.io/qwik")
        || source_contains(source, "@stencil/core")
        || source_contains(source, "preact/")
        || source_contains(source, "'vue'")
        || source_contains(source, "\"vue\"")
        || source_contains(source, "'preact'")
        || source_contains(source, "\"preact\"")
        || has_non_react_jsx_import_source_pragma(source)
        || project.has_non_react_jsx_import_source(path)
}

/// The value of the file's `@jsxImportSource` pragma — the JSX factory package
/// name (`react`, `solid-js`, `hono/jsx`, a relative path, …) — or `None` when
/// the file declares no pragma. The value is the first whitespace-delimited token
/// after the directive; it terminates at whitespace or a comment close (`*/`).
#[must_use]
pub fn jsx_import_source_pragma_value(source: &str) -> Option<&str> {
    let idx = memchr::memmem::find(source.as_bytes(), b"@jsxImportSource")?;
    let after = &source[idx + "@jsxImportSource".len()..];
    let value = after
        .trim_start()
        .split([' ', '\t', '\r', '\n'])
        .next()
        .map(|tok| tok.trim_end_matches("*/"))
        .unwrap_or("");
    (!value.is_empty()).then_some(value)
}

/// True when the file declares a `@jsxImportSource` pragma whose value points to
/// a non-React JSX runtime. Any value other than `react` / `react-dom` (or a
/// `react`/`react-dom` subpath) names a non-React dialect (`hono/jsx`, a relative
/// `./` or `../../src/jsx`, a custom package), which intentionally uses native
/// HTML attribute names and its own `style` semantics. A `react` pragma, or no
/// pragma at all, leaves the file treated as React.
#[must_use]
pub fn has_non_react_jsx_import_source_pragma(source: &str) -> bool {
    jsx_import_source_pragma_value(source).is_some_and(|value| !is_react_jsx_source(value))
}

/// True when the file belongs to a SolidJS project. Unlike
/// [`is_non_react_jsx_file`] — which lumps Solid in with Vue/Preact/Qwik/Stencil
/// to *exempt* React-specific rules — this is a **positive** Solid signal, for
/// rules that must fire only on Solid. Detected three ways: a Solid framework
/// import (`solid-js`, `@solidjs/`, `solid-start`, `@tanstack/solid-router`), a
/// `@jsxImportSource solid-js` pragma, or the nearest `package.json` declaring
/// `solid-js`. Source checks are memoized per file via [`source_contains`].
#[must_use]
pub fn is_solid_file(source: &str, project: &crate::project::ProjectCtx, path: &Path) -> bool {
    source_contains(source, "solid-js")
        || source_contains(source, "@solidjs/")
        || source_contains(source, "solid-start")
        || source_contains(source, "@tanstack/solid-router")
        || jsx_import_source_pragma_value(source).is_some_and(|value| value == "solid-js")
        || project
            .nearest_package_json(path)
            .is_some_and(|pkg| declares_solid(&pkg))
}

/// True when a `package.json` declares `solid-js` in any dependency section.
fn declares_solid(pkg: &crate::project::PackageJson) -> bool {
    pkg.dependencies.contains_key("solid-js")
        || pkg.dev_dependencies.contains_key("solid-js")
        || pkg.peer_dependencies.contains_key("solid-js")
        || pkg.optional_dependencies.contains_key("solid-js")
}

/// True when a `@jsxImportSource` value names React's own runtime: `react`,
/// `react-dom`, or a subpath of either (`react/jsx-runtime`).
fn is_react_jsx_source(value: &str) -> bool {
    value == "react"
        || value == "react-dom"
        || value.starts_with("react/")
        || value.starts_with("react-dom/")
}

/// True if the file is a Web Worker script, where `self` resolves to the
/// `DedicatedWorkerGlobalScope` (the canonical worker global, equivalent to
/// `globalThis` in that realm) rather than `window`. Detected by the
/// worker-only API surface: registering a message handler (`self.onmessage`),
/// posting back to the spawning thread (`self.postMessage(`), the classic
/// worker importer (`importScripts(`), or a reference to the worker global
/// scope type. Memoized per file via [`source_contains`].
#[must_use]
pub fn is_worker_script(source: &str) -> bool {
    cached_file_bool(source, SLOT_WORKER_SCRIPT, || {
        source_contains(source, "self.onmessage")
            || source_contains(source, "self.onmessageerror")
            || source_contains(source, "self.postMessage(")
            || source_contains(source, "importScripts(")
            || source_contains(source, "DedicatedWorkerGlobalScope")
    })
}

/// Pick the right `SourceType` based on file extension. Defaults to `tsx()`
/// for unknown extensions — it's the most permissive (accepts JSX +
/// TypeScript syntax).
pub fn source_type_for_path(path: &Path) -> SourceType {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ts") => SourceType::ts(),
        Some("tsx") => SourceType::tsx(),
        Some("mjs") => SourceType::mjs(),
        Some("cjs") => SourceType::cjs(),
        Some("jsx") => SourceType::jsx(),
        _ => SourceType::tsx(),
    }
}

#[cfg(test)]
pub fn with_semantic<F, R>(source: &str, source_type: SourceType, f: F) -> R
where
    F: for<'a> FnOnce(&'a Semantic<'a>) -> R,
{
    let allocator = Allocator::default();
    let parse_ret = Parser::new(&allocator, source, source_type).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    f(&semantic)
}

/// True when the node starting at byte offset `span_start` is immediately
/// preceded by a `@ts-expect-error` directive comment — only whitespace
/// separates the comment's end from the node. The comment may be a `//` line
/// comment or a `/* … */` block comment; either form carrying the
/// `@ts-expect-error` directive marks the following declaration as a TypeScript
/// error the author has deliberately opted into.
///
/// `comments` is the file's comment list from `semantic.comments()`; using the
/// real comment spans (rather than a raw text scan) keeps a `@ts-expect-error`
/// that merely appears inside a string literal from counting.
pub fn has_ts_expect_error_above(
    comments: &[oxc_ast::ast::Comment],
    source: &str,
    span_start: usize,
) -> bool {
    comments.iter().any(|comment| {
        let end = comment.span.end as usize;
        if end > span_start {
            return false;
        }
        let gap = &source[end..span_start];
        if !gap.chars().all(char::is_whitespace) {
            return false;
        }
        source[comment.span.start as usize..end].contains("@ts-expect-error")
    })
}

/// True when the node starting at byte offset `span_start` is immediately
/// preceded by a JSDoc/leading comment carrying an `@deprecated` tag — only
/// whitespace separates the comment's end from the node. The comment may be a
/// `//` line comment or a `/* … */` (incl. `/** … */`) block comment.
///
/// An `@deprecated` tag marks the declaration's name as part of an external
/// contract retained past its useful body: removing or inlining it is the
/// breaking change the deprecation window exists to defer. Matching against the
/// real comment spans from `semantic.comments()` (rather than a raw text scan)
/// keeps an `@deprecated` that merely appears inside a string literal from
/// counting, and the whitespace-only gap check keeps a far-above comment that
/// belongs to a different declaration from leaking onto this node. The tag is
/// matched case-sensitively, mirroring the sibling `deprecation_without_alternative`
/// rule and the JSDoc canonical lowercase `@deprecated`.
pub fn node_has_preceding_deprecated_tag(
    comments: &[oxc_ast::ast::Comment],
    source: &str,
    span_start: usize,
) -> bool {
    comments.iter().any(|comment| {
        let end = comment.span.end as usize;
        if end > span_start {
            return false;
        }
        let gap = &source[end..span_start];
        if !gap.chars().all(char::is_whitespace) {
            return false;
        }
        source[comment.span.start as usize..end].contains("@deprecated")
    })
}

/// Convert an oxc byte offset into 1-based `(line, column)`.
///
/// Shared across all `OxcCheck` rules that emit diagnostics. Rules call this
/// once per emitted diagnostic, so a naive scan from the start of the file
/// costs O(byte_offset) per call — quadratic on files that emit many
/// diagnostics. Instead we build a per-file index of line-start offsets once
/// (cached in a thread-local, keyed by the source `(ptr, len)` identity) and
/// binary-search it: O(log lines) to find the line, then O(line length) to
/// count the column (chars, skipping `\r`).
pub fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    LINE_INDEX.with(|c| {
        let mut c = c.borrow_mut();
        let ptr = source.as_ptr() as usize;
        let len = source.len();
        if c.ptr != ptr || c.len != len {
            c.ptr = ptr;
            c.len = len;
            c.starts.clear();
            c.starts.push(0);
            for (i, b) in source.bytes().enumerate() {
                if b == b'\n' {
                    c.starts.push(i + 1);
                }
            }
        }
        // Largest line start <= byte_offset (`starts[0] == 0 <= byte_offset`).
        let line_idx = match c.starts.binary_search(&byte_offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let line_start = c.starts[line_idx];
        let mut col = 1;
        for (i, ch) in source[line_start..].char_indices() {
            if line_start + i >= byte_offset {
                break;
            }
            if ch != '\r' {
                col += 1;
            }
        }
        (line_idx + 1, col)
    })
}

/// Return a copy of `source` with the *contents* of `//` line comments and
/// `/* … */` (incl. `/** … */` JSDoc) block comments replaced by spaces, so a
/// text needle that appears only in a comment is no longer found.
///
/// The mask is **offset-preserving**: every comment byte becomes a single
/// space except `\n`, which is kept verbatim. The result has the same byte
/// length and the same newline positions as `source`, so a byte offset into
/// the masked string maps to the same `(line, column)` via
/// [`byte_offset_to_line_col`] as it would in the original. Multibyte UTF-8
/// inside a comment is replaced byte-for-byte with spaces, which stays valid
/// ASCII and preserves length.
///
/// String and template literals are skipped so a `//` or `/*` *inside* a
/// string is not mistaken for the start of a comment. This is the central
/// remedy for the "needle matched inside a comment" false-positive class that
/// affects `TextCheck` rules.
#[must_use]
pub fn mask_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'"' | b'\'' | b'`' => {
                let quote = bytes[i];
                i += 1;
                while i < len {
                    match bytes[i] {
                        b'\\' => i += 2,
                        b if b == quote => {
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                while i < len && bytes[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                out[i] = b' ';
                out[i + 1] = b' ';
                i += 2;
                while i < len && !(bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/') {
                    if bytes[i] != b'\n' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
                if i < len {
                    out[i] = b' ';
                    if i + 1 < len {
                        out[i + 1] = b' ';
                    }
                    i += 2;
                }
            }
            _ => i += 1,
        }
    }
    // Every replacement writes an ASCII space over a byte that was already a
    // standalone ASCII byte (`/`, `*`) or one byte of a fully-masked multibyte
    // sequence, so char boundaries are never split and the buffer stays UTF-8.
    String::from_utf8(out).expect("mask_comments only writes ASCII spaces, output stays valid UTF-8")
}

/// Parse `source` with oxc_parser using the source type inferred from `path`,
/// build semantic analysis, then hand the `Semantic` to `f`. The allocator
/// and AST are dropped after `f` returns.
///
/// Used by the engine hot path for `Backend::Oxc` dispatch.
pub fn with_oxc_parse<F, R>(source: &str, path: &Path, f: F) -> R
where
    F: for<'a> FnOnce(&'a Semantic<'a>) -> R,
{
    let source_type = source_type_for_path(path);
    let allocator = Allocator::default();
    let parse_ret = Parser::new(&allocator, source, source_type).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    f(&semantic)
}

/// TanStack Query / Solid Query / Vue Query factory calls whose options
/// object accepts callbacks with library-dictated signatures (`onError`
/// gets `(error, variables, context, mutation)`, `queryFn` gets a context
/// object, etc.). When the user writes those callbacks they have no say
/// over the arity — flagging them with `max-params` is a guaranteed false
/// positive.
const TANSTACK_QUERY_FACTORIES: &[&str] = &[
    "useMutation",
    "useQuery",
    "useInfiniteQuery",
    "useQueries",
    "useSuspenseQuery",
    "useSuspenseInfiniteQuery",
    "useSuspenseQueries",
    "createMutation",
    "createQuery",
    "createInfiniteQuery",
    // Per-call callback options on the mutation result object: these accept
    // the same fixed-signature callbacks as the factory options object.
    "mutate",
    "mutateAsync",
    // Cache constructors — `new MutationCache({ onError })` /
    // `new QueryCache({ onError })` take the same fixed-signature callbacks.
    "MutationCache",
    "QueryCache",
];

/// Option-keys inside a TanStack Query factory call whose value is a
/// callback with a fixed signature dictated by the library types.
const TANSTACK_QUERY_CALLBACK_KEYS: &[&str] = &[
    "onError",
    "onSuccess",
    "onSettled",
    "onMutate",
    "mutationFn",
    "queryFn",
    "getNextPageParam",
    "getPreviousPageParam",
];

/// True when `node` is a function expression / arrow function being passed
/// as a known third-party callback whose signature is dictated by the
/// outer call's type — e.g. `useMutation({ onError: (a, b, c, d) => ... })`.
///
/// Recognises:
/// 1. `node` is the value of an object property in an object literal.
/// 2. That object literal is one of the arguments of a CallExpression
///    (any position — TanStack Query v4 accepts
///    `useQuery(queryKey, queryFn, options)`).
/// 3. The CallExpression's callee identifier is one of
///    [`TANSTACK_QUERY_FACTORIES`].
/// 4. The property name is one of [`TANSTACK_QUERY_CALLBACK_KEYS`].
///
/// All four must hold. The match is purely name-based — hand-rolled
/// look-alikes are out of scope (the user can rename their helper or open
/// an issue to add it to the allowlist).
#[must_use]
pub fn is_fixed_signature_library_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Expression, PropertyKey};

    let nodes = semantic.nodes();
    let node_span = {
        use oxc_span::GetSpan;
        match node.kind() {
            AstKind::Function(f) => f.span(),
            AstKind::ArrowFunctionExpression(a) => a.span(),
            _ => return false,
        }
    };

    // Walk up to the enclosing ObjectProperty.
    let mut current_id = node.id();
    let object_property_key: &str;
    let object_property_node_id;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        if let AstKind::ObjectProperty(prop) = parent.kind() {
            // The function must be the property's *value*, not nested
            // somewhere deeper (e.g. a default expression).
            use oxc_span::GetSpan;
            let value_span = prop.value.span();
            if value_span != node_span {
                return false;
            }
            object_property_key = match &prop.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => return false,
            };
            object_property_node_id = parent_id;
            break;
        }
        current_id = parent_id;
    }

    if !TANSTACK_QUERY_CALLBACK_KEYS.contains(&object_property_key) {
        return false;
    }

    // The property's parent must be an ObjectExpression that is the first
    // argument of a CallExpression whose callee identifier is in the
    // factory allowlist.
    let obj_parent_id = nodes.parent_id(object_property_node_id);
    if obj_parent_id == object_property_node_id {
        return false;
    }
    let obj_parent = nodes.get_node(obj_parent_id);
    let AstKind::ObjectExpression(obj_expr) = obj_parent.kind() else {
        return false;
    };

    let call_parent_id = nodes.parent_id(obj_parent_id);
    if call_parent_id == obj_parent_id {
        return false;
    }
    let call_parent = nodes.get_node(call_parent_id);
    // The options object may be an argument of either a call (`useMutation({…})`)
    // or a constructor (`new MutationCache({…})`).
    let (callee, arguments) = match call_parent.kind() {
        AstKind::CallExpression(call) => (&call.callee, &call.arguments),
        AstKind::NewExpression(new_expr) => (&new_expr.callee, &new_expr.arguments),
        _ => return false,
    };

    // Any argument may be this ObjectExpression — TanStack Query v4 supports
    // the overloaded `useQuery(queryKey, queryFn, options)` shape where the
    // options object is the third argument.
    use oxc_span::GetSpan;
    let obj_expr_span = obj_expr.span();
    let matches_any_arg = arguments.iter().any(|arg| {
        arg.as_expression()
            .is_some_and(|expr| expr.span() == obj_expr_span)
    });
    if !matches_any_arg {
        return false;
    }

    // Callee identifier in allowlist. Handles both bare calls (`useMutation`)
    // and namespace-import calls (`RQ.useMutation`) — the receiver is not
    // verified to be a namespace import; property name is sufficient.
    let callee_name = match callee {
        Expression::Identifier(id) => id.name.as_str(),
        Expression::StaticMemberExpression(member) => member.property.name.as_str(),
        _ => return false,
    };
    TANSTACK_QUERY_FACTORIES.contains(&callee_name)
}

/// True when `ident` resolves to a local binding declared with `const` or `let`
/// whose initializer is a plain object literal or object-spread
/// (`Expression::ObjectExpression`, which covers both `{ key: val }` and
/// `{ ...other }`). Such a binding is a freshly-created local builder, not a
/// reference to shared state: assigning its properties (`value.x = ...`) or
/// deleting them (`delete value.x`) before returning it is the object analogue
/// of the `const items = []; items.push(x)` accumulator pattern, and mutates no
/// external state.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, then
/// inspects the `VariableDeclarator` (whose `kind` carries the declaration
/// keyword). A function parameter, imported binding, or `this` resolves to a
/// non-`VariableDeclarator` declaration; a `var` binding or a non-object-literal
/// initializer is rejected, so any mutation through it is still flagged.
#[must_use]
pub fn is_local_object_builder_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Expression, VariableDeclarationKind};

    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in
        std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            return matches!(
                decl.kind,
                VariableDeclarationKind::Const | VariableDeclarationKind::Let
            ) && matches!(decl.init, Some(Expression::ObjectExpression(_)));
        }
    }
    false
}

/// True when `expr` is a primitive literal — `string`, `number`, or `boolean` —
/// or an identifier whose binding's initializer is directly such a literal
/// (`const k = "abc"; … x === k`). A binding initialized from a call
/// (`getSecret()`), a member access (`process.env.KEY`), or a compound
/// expression (`"a" + x`) is **not** a literal and returns `false`, so a stored
/// secret stays distinguishable from an inline constant.
///
/// For timing-attack rules, comparing against a literal whose bytes are present
/// in the source — inline or behind one level of `const` indirection — leaks
/// nothing an attacker cannot already read.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, then
/// inspects the enclosing `VariableDeclarator`'s `init`. Only a *direct* literal
/// initializer matches; a literal nested inside a larger expression does not.
#[must_use]
pub fn expression_is_or_resolves_to_literal(
    expr: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;

    if is_primitive_literal(expr) {
        return true;
    }
    let Expression::Identifier(ident) = expr else {
        return false;
    };
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in
        std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            return decl.init.as_ref().is_some_and(is_primitive_literal);
        }
    }
    false
}

/// True when `expr` is directly a `string`, `number`, or `boolean` literal.
fn is_primitive_literal(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    matches!(
        expr,
        Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
    )
}

/// True when `ident` resolves to the setter slot of a React state hook — the
/// second element of an array-destructuring binding whose initializer is a
/// `useState` or `useReducer` call (`const [value, setValue] = useState(...)`).
/// These are the only identifiers that schedule a React render when called;
/// every other `set`-prefixed callee (`setTimeout`, `setInterval`, `setHeaders`,
/// …) resolves to a different declaration shape, or to no local binding at all,
/// and is rejected.
///
/// Matches on the **setter** slot only: the state slot may be a destructuring
/// hole (`const [, setValue] = useState(...)`), which still binds a render-
/// scheduling setter and is therefore recognized.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, then
/// confirms the declaration is a `VariableDeclarator` whose id is an
/// `ArrayPattern` whose second slot is this identifier and whose initializer
/// calls `useState`/`useReducer` (bare or member-qualified, e.g. `React.useState`).
#[must_use]
pub fn is_use_state_setter_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use_state_setter_array_pattern(ident, semantic).is_some()
}

/// Resolves a `useState`/`useReducer` setter `IdentifierReference` to the name of
/// the **state variable** it is paired with — the first element of the
/// destructuring `ArrayPattern` whose second element is this setter
/// (`const [value, setValue] = useState(...)` → `"value"`). Returns `None` for any
/// identifier that is not such a setter, or for a destructure whose state slot is
/// not a plain binding identifier (`const [, setValue] = ...`).
///
/// This is the pairing the "adjust state during render" guard needs: a setter call
/// guarded by `if (cond && state === x)` only terminates when the guard test
/// references the *paired* state variable, so the exemption must match this exact
/// state name and no other identifier. Shares the setter-slot locator with
/// [`is_use_state_setter_binding`] and additionally reads slot 0.
#[must_use]
pub fn use_state_setter_state_name(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> Option<String> {
    use oxc_ast::ast::BindingPattern;

    let arr = use_state_setter_array_pattern(ident, semantic)?;
    match arr.elements.first() {
        Some(Some(BindingPattern::BindingIdentifier(state_id))) => {
            Some(state_id.name.as_str().to_string())
        }
        _ => None,
    }
}

/// Locate the `useState`/`useReducer` destructuring `ArrayPattern` for which
/// `ident` is the **setter** (slot 1). Returns the pattern so callers can read
/// the paired state slot (slot 0) when they need it; the setter match itself
/// never inspects slot 0, so a state-slot hole (`const [, setValue] = ...`) is
/// still recognized.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, finds the
/// enclosing `VariableDeclarator`, confirms its initializer calls
/// `useState`/`useReducer` (bare or member-qualified, e.g. `React.useState`), and
/// requires slot 1 of the destructure to be a `BindingIdentifier` named like `ident`.
fn use_state_setter_array_pattern<'a>(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a oxc_ast::ast::ArrayPattern<'a>> {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{BindingPattern, Expression};
    use oxc_span::GetSpan;

    let ref_id = ident.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in
        std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            let Some(Expression::CallExpression(call)) = &decl.init else {
                return None;
            };
            let callee_span = call.callee.span();
            let callee_text = semantic.source_text()
                [callee_span.start as usize..callee_span.end as usize]
                .rsplit('.')
                .next()
                .unwrap_or("");
            if callee_text != "useState" && callee_text != "useReducer" {
                return None;
            }
            let BindingPattern::ArrayPattern(arr) = &decl.id else {
                return None;
            };
            // Slot 1 must be this setter identifier; slot 0 (the paired state) is
            // intentionally not inspected, so a state-slot hole still matches.
            let is_setter = matches!(
                arr.elements.get(1),
                Some(Some(BindingPattern::BindingIdentifier(setter_id)))
                    if setter_id.name == ident.name
            );
            return is_setter.then_some(arr.as_ref());
        }
    }
    None
}

/// True when the setter call at `call_node_id` is nested under an `IfStatement`
/// (or `ConditionalExpression`) whose **test references one of `state_names`** —
/// the React-sanctioned "adjust state during render" pattern
/// (<https://react.dev/reference/react/useState#storing-information-from-previous-renders>).
///
/// Such a guard terminates: `if (isOpen && state === 'closed') setState('open')`
/// re-renders only until `state` matches, then the condition is false and React
/// bails out — no infinite loop. The pairing must be precise: `state_names` are the
/// state variables paired with *this* setter, so a guard on an unrelated flag
/// (`if (someProp) setState(x)`) is NOT exempted and stays flagged.
///
/// Walks up the `parent_id` chain from `call_node_id`, stopping at `boundary_id`
/// (the render-function node, exclusive). Any enclosing `IfStatement` /
/// `ConditionalExpression` counts — the call need not be in the consequent/alternate
/// specifically (a setter in the test itself is degenerate and out of scope). The
/// match is by identifier *name*, not resolved symbol: a guard test mentioning a
/// shadowing local of the same name would also match, which only ever *widens* the
/// exemption (never a new false positive).
#[must_use]
pub fn is_guarded_derive_during_render(
    call_node_id: oxc_semantic::NodeId,
    state_names: &FxHashSet<String>,
    boundary_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;

    if state_names.is_empty() {
        return false;
    }
    let nodes = semantic.nodes();
    let mut cur = call_node_id;
    loop {
        let parent_id = nodes.parent_id(cur);
        if parent_id == cur || parent_id == boundary_id {
            return false;
        }
        let test_span = match nodes.kind(parent_id) {
            AstKind::IfStatement(stmt) => stmt.test.span(),
            AstKind::ConditionalExpression(cond) => cond.test.span(),
            _ => {
                cur = parent_id;
                continue;
            }
        };
        if test_references_state(test_span, state_names, semantic) {
            return true;
        }
        cur = parent_id;
    }
}

/// True when any `IdentifierReference` inside `test_span` names one of
/// `state_names`. Scans semantic nodes by span containment within the guard test.
fn test_references_state(
    test_span: oxc_span::Span,
    state_names: &FxHashSet<String>,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;

    semantic.nodes().iter().any(|node| {
        if let AstKind::IdentifierReference(id) = node.kind() {
            let span = id.span();
            test_span.start <= span.start
                && span.end <= test_span.end
                && state_names.contains(id.name.as_str())
        } else {
            false
        }
    })
}

/// True when `ident` resolves to the **accumulator** parameter of an
/// `Array.prototype.reduce` callback — the first parameter of an arrow or
/// function expression passed as the first argument to a `.reduce(...)` call.
///
/// The seed (`reduce`'s second argument) is a fresh value created at the call
/// site, so the accumulator is a local builder that never escapes until `reduce`
/// returns. Mutating its properties (`acc[key] = v`) is the canonical
/// reduce-to-object pattern and mutates no external state — the object analogue
/// of `const items = []; items.push(x)`.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, then
/// confirms the declaration is the first formal parameter of a function/arrow
/// whose parent is a `CallExpression` keyed by a `.reduce` member and that takes
/// the function as its first argument. Any other parameter, or the accumulator
/// of a non-`reduce` call, resolves to a different shape and stays flagged.
#[must_use]
pub fn is_reduce_accumulator_param(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;
    use oxc_span::GetSpan;

    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    let decl_span = nodes.kind(decl_node_id).span();

    // Walk from the binding's declaration up to the function it parameterises.
    // Require the binding to be that function's first formal parameter, and the
    // function to be the first argument of a `.reduce(...)` call.
    let mut is_first_param = false;
    for ancestor in nodes.ancestors(decl_node_id) {
        match ancestor.kind() {
            AstKind::FormalParameters(params) => {
                is_first_param = params
                    .items
                    .first()
                    .is_some_and(|first| first.span.start <= decl_span.start && decl_span.end <= first.span.end);
                if !is_first_param {
                    return false;
                }
            }
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                if !is_first_param {
                    return false;
                }
                let AstKind::CallExpression(call) = nodes.parent_node(ancestor.id()).kind() else {
                    return false;
                };
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return false;
                };
                if member.property.name.as_str() != "reduce" {
                    return false;
                }
                let fn_span = ancestor.kind().span();
                return call
                    .arguments
                    .first()
                    .and_then(|arg| arg.as_expression())
                    .is_some_and(|arg| arg.span() == fn_span);
            }
            _ => {}
        }
    }
    false
}

/// True when `ident` resolves to a binding initialised from a `.getContext(...)`
/// call — e.g. `const ctx = canvas.getContext('2d')`. A rendering context
/// (`CanvasRenderingContext2D`, `WebGLRenderingContext`, …) is an imperative,
/// stateful browser API whose entire contract is property assignment
/// (`ctx.fillStyle = …`, `ctx.lineWidth = …`); there is no immutable
/// "build a new context" alternative. Mutating its properties is the API.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, then
/// confirms the declarator's initializer is a call to a `.getContext` member,
/// unwrapping a trailing non-null assertion (`getContext('2d')!`). Any other
/// initializer shape stays flagged.
#[must_use]
pub fn is_get_context_call_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;

    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in
        std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            let Some(init) = &decl.init else {
                return false;
            };
            return is_get_context_call(init);
        }
    }
    false
}

/// True when `expr` is a `*.getContext(...)` call, looking through a trailing
/// non-null assertion (`canvas.getContext('2d')!`).
fn is_get_context_call(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;

    match expr {
        Expression::TSNonNullExpression(nn) => is_get_context_call(&nn.expression),
        Expression::CallExpression(call) => {
            matches!(
                &call.callee,
                Expression::StaticMemberExpression(member)
                    if member.property.name.as_str() == "getContext"
            )
        }
        _ => false,
    }
}

/// Vue 3 reactive factories whose return value is a `Ref<T>` wrapper mutated
/// through its `.value` property. `customRef` and (writable) `computed` follow
/// the same `ref.value = x` contract.
const VUE_REF_FACTORIES: &[&str] = &["ref", "shallowRef", "customRef", "computed"];

/// True when `ident` resolves to a `const`/`let` binding initialised by a Vue
/// reactive factory imported from `vue` — `ref(...)`, `shallowRef(...)`,
/// `customRef(...)`, or `computed(...)`. Such a binding is a `Ref<T>` wrapper
/// whose `.value` property is the *intended* mutation point: assigning
/// `count.value = x` / `count.value++` is how Vue's reactivity is driven, not a
/// mutation of a plain object. Callers gate the `.value` property specifically;
/// any other property write on the ref stays flagged.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, then
/// confirms the declarator initializer is a call to one of the factory names and
/// that the callee identifier is imported from `vue` (so a same-named local
/// `ref()` is not mistaken for Vue's).
#[must_use]
pub fn is_vue_ref_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;

    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in
        std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            let Some(init) = &decl.init else {
                return false;
            };
            let Expression::CallExpression(call) = init else {
                return false;
            };
            let Expression::Identifier(callee) = &call.callee else {
                return false;
            };
            let name = callee.name.as_str();
            return VUE_REF_FACTORIES.contains(&name) && is_imported_from_vue(name, semantic);
        }
    }
    false
}

/// True when `member` is a `<ref>.value` access where `<ref>` is a direct
/// identifier bound to a Vue reactive factory (`ref`/`shallowRef`/`customRef`/
/// `computed` imported from `vue`). This is the idiomatic Vue 3 reactive-update
/// target: `count.value = x` / `count.value++`. Restricted to the `value`
/// property and a direct-identifier base, so `ref.config = x` and `a.b.value = x`
/// stay flagged.
#[must_use]
pub fn is_vue_ref_value_target(
    member: &oxc_ast::ast::StaticMemberExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::Expression;

    if member.property.name.as_str() != "value" {
        return false;
    }
    let Expression::Identifier(base) = &member.object else {
        return false;
    };
    is_vue_ref_binding(base, semantic)
}

/// True when `local_name` is the local binding of a named import from `vue`
/// (`import { ref } from 'vue'`).
fn is_imported_from_vue(local_name: &str, semantic: &oxc_semantic::Semantic) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::ImportDeclarationSpecifier;

    semantic.nodes().iter().any(|node| {
        let AstKind::ImportDeclaration(decl) = node.kind() else {
            return false;
        };
        if decl.source.value.as_str() != "vue" {
            return false;
        }
        let Some(specifiers) = &decl.specifiers else {
            return false;
        };
        specifiers.iter().any(|spec| match spec {
            ImportDeclarationSpecifier::ImportSpecifier(named) => {
                named.local.name.as_str() == local_name
            }
            _ => false,
        })
    })
}

/// True when `assign` sets a `displayName` property to a string literal
/// (`Component.displayName = "Component"`). React reads `displayName` off the
/// component function object to name anonymous `forwardRef`/`memo` results in
/// DevTools, error messages, and stack traces — a metadata API with no
/// immutable alternative, not a state-mutation smell. Restricted to a string\
/// literal RHS so other `displayName` writes stay flagged.
#[must_use]
pub fn is_react_display_name_assignment(assign: &oxc_ast::ast::AssignmentExpression) -> bool {
    use oxc_ast::ast::{AssignmentTarget, Expression};

    let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
        return false;
    };
    member.property.name.as_str() == "displayName"
        && matches!(&assign.right, Expression::StringLiteral(_))
}

/// True when `name` matches a generic type parameter declared on any enclosing
/// function, method, class, interface, or type alias of `node`.
#[must_use]
pub fn name_is_generic_type_param_in_scope(
    name: &str,
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    for ancestor in semantic.nodes().ancestors(node_id) {
        let type_params = match ancestor.kind() {
            AstKind::Function(f) => f.type_parameters.as_deref(),
            AstKind::ArrowFunctionExpression(a) => a.type_parameters.as_deref(),
            AstKind::Class(c) => c.type_parameters.as_deref(),
            AstKind::TSInterfaceDeclaration(i) => i.type_parameters.as_deref(),
            AstKind::TSTypeAliasDeclaration(a) => a.type_parameters.as_deref(),
            AstKind::TSMethodSignature(m) => m.type_parameters.as_deref(),
            AstKind::TSCallSignatureDeclaration(c) => c.type_parameters.as_deref(),
            AstKind::TSConstructSignatureDeclaration(c) => c.type_parameters.as_deref(),
            _ => None,
        };
        if let Some(tp_decl) = type_params {
            for tp in &tp_decl.params {
                if tp.name.name.as_str() == name {
                    return true;
                }
            }
        }
    }
    false
}

/// True when the `.then()` `call` node is the direct expression body of a
/// non-async arrow function that is passed as an argument to `lazy()` or
/// `React.lazy()`.
///
/// `React.lazy()` requires a **synchronous** factory that returns a Promise —
/// passing an `async` arrow would violate the spec. The idiomatic module-
/// reshaping pattern `lazy(() => import("...").then(mod => ({ default: mod.X })))`
/// is therefore the only valid form, and flagging its `.then()` is a false
/// positive.
///
/// The check is purely syntactic (callee name `"lazy"`); it does not resolve
/// imports, so hand-rolled helpers that happen to be named `lazy` are also
/// exempted — an acceptable trade-off for zero false positives on the real pattern.
#[must_use]
pub fn is_react_lazy_factory_then<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;

    let mut found_expression_arrow = false;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(a) => {
                if a.r#async || !a.expression {
                    return false;
                }
                found_expression_arrow = true;
            }
            AstKind::Function(_) if found_expression_arrow => return false,
            AstKind::CallExpression(call) if found_expression_arrow => {
                let callee_name = match &call.callee {
                    Expression::Identifier(id) => id.name.as_str(),
                    Expression::StaticMemberExpression(m) => m.property.name.as_str(),
                    _ => return false,
                };
                return callee_name == "lazy";
            }
            _ => {}
        }
    }
    false
}

/// Method names of the Playwright/Puppeteer APIs that serialize a function
/// argument and run it inside the browser page realm.
const BROWSER_EVAL_METHODS: &[&str] = &["evaluate", "evaluateHandle", "$eval", "$$eval"];

/// True when `node` is lexically inside the function argument of a
/// `*.evaluate(...)` / `*.evaluateHandle(...)` / `*.$eval(...)` / `*.$$eval(...)`
/// call (Playwright/Puppeteer browser-context-injection APIs).
///
/// The callback passed to these methods is serialized and executed in the
/// browser page realm, where `window` is the canonical global object — not the
/// cross-realm `globalThis`. Rules that prefer `globalThis` over `window` must
/// stay silent inside such callbacks.
///
/// Detection is by the callee's property name only; the receiver (page handle,
/// frame, element handle, …) is not constrained, since these methods exist on
/// several Playwright/Puppeteer handle types under the same names.
#[must_use]
pub fn is_in_browser_eval_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;
    use oxc_span::GetSpan;

    // The nearest enclosing function is the candidate callback. We record its
    // span so that, on reaching the enclosing call, we can confirm the function
    // is the call's *argument* and not (say) its receiver.
    let mut enclosing_fn_span: Option<oxc_span::Span> = None;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
                if enclosing_fn_span.is_none() =>
            {
                enclosing_fn_span = Some(ancestor.kind().span());
            }
            AstKind::CallExpression(call) => {
                let Some(fn_span) = enclosing_fn_span else {
                    continue;
                };
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    continue;
                };
                if !BROWSER_EVAL_METHODS.contains(&member.property.name.as_str()) {
                    continue;
                }
                // The callback must be one of the call's arguments, not the
                // receiver: `window.foo` inside `evaluate.call(...)` style code
                // would otherwise be wrongly exempted.
                let is_argument = call
                    .arguments
                    .iter()
                    .filter_map(|arg| arg.as_expression())
                    .any(|arg| arg.span() == fn_span);
                if is_argument {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Which of `window`/`self`/`global` the file feature-detects via a `typeof`
/// check (`typeof window !== "undefined"`, `typeof self`, …). A file that probes
/// for a global before using it is deliberately writing environment-aware code:
/// the guarded alias is the intended object there (a browser-only library uses
/// `window` on purpose), so `prefer-global-this` must not push `globalThis` onto
/// those accesses. Scans the semantic tree once per file (memoized by source
/// `(ptr, len)`) since `OxcCheck::run` queries it from every `window.X` node.
#[must_use]
pub fn file_typeof_guards<'a>(
    source: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> TypeofGuards {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Expression, UnaryOperator};

    let ptr = source.as_ptr() as usize;
    let len = source.len();
    let cur = TYPEOF_GUARDS.with(std::cell::Cell::get);
    if cur.ptr == ptr && cur.len == len && let Some(guards) = cur.guards {
        return guards;
    }

    let mut guards = TypeofGuards::default();
    for node in semantic.nodes().iter() {
        let AstKind::UnaryExpression(unary) = node.kind() else {
            continue;
        };
        if unary.operator != UnaryOperator::Typeof {
            continue;
        }
        let Expression::Identifier(id) = &unary.argument else {
            continue;
        };
        match id.name.as_str() {
            "window" => guards.window = true,
            "self" => guards.self_ = true,
            "global" => guards.global = true,
            _ => {}
        }
        if guards.window && guards.self_ && guards.global {
            break;
        }
    }

    TYPEOF_GUARDS.with(|c| {
        c.set(TypeofGuardSlot {
            ptr,
            len,
            guards: Some(guards),
        });
    });
    guards
}

/// True when `node_id` sits inside an ambient (`declare`) module context —
/// `declare global { ... }` (parsed as `TSGlobalDeclaration`) or a `declare`
/// module/namespace (`TSModuleDeclaration` with `declare`). Bindings inside
/// these blocks are type-level ambient declarations with no runtime presence,
/// so runtime-variable lints must not fire on them.
#[must_use]
pub fn is_in_ambient_declaration(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    semantic.nodes().ancestors(node_id).any(|ancestor| {
        matches!(ancestor.kind(), AstKind::TSGlobalDeclaration(_))
            || matches!(ancestor.kind(), AstKind::TSModuleDeclaration(m) if m.declare)
    })
}

/// True when `node_id` sits inside a TypeScript namespace/module body
/// (`namespace Foo { … }` or `module Foo { … }`) — i.e. some strict ancestor is
/// a `TSModuleDeclaration`. A namespace is its own scope: `export interface X`
/// inside it is reachable only as `Foo.X`, never as a module-level binding, so
/// two namespaces may each export an `X` without clashing. Callers that reason
/// about module-level exports use this to exclude namespace-scoped members.
#[must_use]
pub fn is_in_ts_namespace(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    semantic
        .nodes()
        .ancestors(node_id)
        .any(|ancestor| matches!(ancestor.kind(), AstKind::TSModuleDeclaration(_)))
}

/// Walk up from `node_id` to its nearest enclosing `Class`, returning the class
/// AST node. Stops at the first `Class` ancestor (a method's own class), or
/// `None` if the node has no enclosing class.
#[must_use]
pub fn enclosing_class<'a>(
    node_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes<'a>,
) -> Option<&'a oxc_ast::ast::Class<'a>> {
    use oxc_ast::AstKind;
    let mut current = node_id;
    loop {
        let parent_id = nodes.parent_id(current);
        if parent_id == current {
            return None;
        }
        let parent = nodes.get_node(parent_id);
        if let AstKind::Class(class) = parent.kind() {
            return Some(class);
        }
        current = parent_id;
    }
}

/// The heritage/decorator shape of a class, with each axis exposed separately so
/// callers exempt on exactly the axis they care about. `has_super_class`
/// (`extends Base`) and `has_implements` (`implements I`) are kept distinct
/// rather than bundled: rules that only care about `extends` must not also
/// exempt on `implements`, which would introduce false negatives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassShape {
    pub is_decorated: bool,
    pub has_super_class: bool,
    pub has_implements: bool,
}

impl ClassShape {
    #[must_use]
    pub fn of(class: &oxc_ast::ast::Class) -> ClassShape {
        ClassShape {
            is_decorated: !class.decorators.is_empty(),
            has_super_class: class.super_class.is_some(),
            has_implements: !class.implements.is_empty(),
        }
    }
}

/// True when `decorator_name` is a class decorator that registers its class in
/// the browser's custom-element registry as a side effect (Lit's
/// `@customElement('tag')`, which calls `customElements.define(...)`). Such a
/// class is reached through its HTML tag name, never through a JavaScript
/// identifier reference, so usage- and reachability-based rules must treat the
/// decorated class as live even with no import or in-file reference.
///
/// Matched on the decorator's callee identifier only; the registered tag string
/// is irrelevant. Add registering decorator names here so both `ts-no-unused-vars`
/// and `dead-export` stay in sync from one place.
#[must_use]
pub fn is_custom_element_decorator_name(decorator_name: &str) -> bool {
    decorator_name == "customElement"
}

/// Peel any nested `ParenthesizedExpression` wrappers off `expr`, returning the
/// first non-parenthesized inner expression. Used by the cast rules so that
/// `(x as unknown) as T` is analyzed identically to `x as unknown as T`.
#[must_use]
pub fn peel_parens<'a>(
    expr: &'a oxc_ast::ast::Expression<'a>,
) -> &'a oxc_ast::ast::Expression<'a> {
    use oxc_ast::ast::Expression;
    let mut current = expr;
    while let Expression::ParenthesizedExpression(p) = current {
        current = &p.expression;
    }
    current
}

/// True when `object` (the receiver of a `<object>.postMessage(...)` call) is a
/// window-like reference that accepts a cross-origin `targetOrigin` argument.
///
/// Only `Window.postMessage(message, targetOrigin)` and the cross-window forms
/// (`iframe.contentWindow`, `window.open(...)` result, `parent`/`top`/`opener`)
/// take a `targetOrigin`; `BroadcastChannel`, `Worker`, `MessagePort`, and the
/// worker `DedicatedWorkerGlobalScope` expose a one-argument `postMessage` with
/// no such parameter. The target-origin rules must therefore only inspect a
/// window-like receiver, otherwise they flag those same-origin-by-design APIs.
///
/// `self` and `globalThis` are deliberately excluded: in a worker script they
/// resolve to `DedicatedWorkerGlobalScope`, whose `postMessage(message, transfer)`
/// has no `targetOrigin`. They are genuinely ambiguous without type information,
/// and cross-origin messaging never targets the current realm, so treating them
/// as window-like only produces false positives on worker globals.
///
/// Recognised as window-like:
///  - identifiers `window`, `parent`, `top`, `opener`;
///  - any member access ending in `.contentWindow` (`iframe.contentWindow`),
///    or in `.parent`/`.top`/`.opener`/`.self`/`.window` (window navigators);
///  - a `window.open(...)`/`open(...)` call result.
///
/// Any other receiver (a `BroadcastChannel`/`Worker`/`MessagePort` instance,
/// `self`/`globalThis` worker globals, `this.channel`, an arbitrary local
/// binding, `new BroadcastChannel(...)`) is not window-like and is left
/// unflagged.
#[must_use]
pub fn is_window_like_post_message_target(object: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;

    const WINDOW_IDENTS: &[&str] = &["window", "parent", "top", "opener"];
    const WINDOW_PROPERTIES: &[&str] =
        &["contentWindow", "parent", "top", "opener", "self", "window"];

    match peel_parens(object) {
        Expression::Identifier(id) => WINDOW_IDENTS.contains(&id.name.as_str()),
        Expression::ThisExpression(_) => false,
        Expression::StaticMemberExpression(member) => {
            WINDOW_PROPERTIES.contains(&member.property.name.as_str())
        }
        Expression::CallExpression(call) => match peel_parens(&call.callee) {
            Expression::Identifier(id) => id.name.as_str() == "open",
            Expression::StaticMemberExpression(member) => member.property.name.as_str() == "open",
            _ => false,
        },
        _ => false,
    }
}

/// True when `as_expr` is the **outer** half of an `x as unknown as T` chain —
/// its inner expression (after peeling parentheses) is itself a `TSAsExpression`
/// whose target is the `unknown` keyword. This is the canonical
/// contravariant-boundary escape hatch; the outer cast is then not a narrowing.
#[must_use]
pub fn is_outer_as_unknown_double_cast(as_expr: &oxc_ast::ast::TSAsExpression) -> bool {
    use oxc_ast::ast::{Expression, TSType};
    matches!(
        peel_parens(&as_expr.expression),
        Expression::TSAsExpression(inner) if matches!(inner.type_annotation, TSType::TSUnknownKeyword(_))
    )
}

/// True when `as_expr` participates in an `x as unknown as T` chain on **either**
/// half:
///  - the outer half (its inner is `_ as unknown`, see
///    [`is_outer_as_unknown_double_cast`]); or
///  - the inner `_ as unknown` half whose parent chain (walking past
///    `ParenthesizedExpression` wrappers) reaches an enclosing `TSAsExpression`.
///
/// `ts-no-as-narrowing` exempts only the outer half, so it must use
/// [`is_outer_as_unknown_double_cast`]; `no-type-assertion` exempts both halves
/// and uses this.
#[must_use]
pub fn is_as_unknown_double_cast(
    node_id: oxc_semantic::NodeId,
    as_expr: &oxc_ast::ast::TSAsExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::TSType;

    if is_outer_as_unknown_double_cast(as_expr) {
        return true;
    }

    // Inner half: `_ as unknown` whose parent (past parentheses) is another
    // TSAsExpression.
    if matches!(as_expr.type_annotation, TSType::TSUnknownKeyword(_)) {
        let nodes = semantic.nodes();
        let mut cur = node_id;
        loop {
            let parent_id = nodes.parent_id(cur);
            if parent_id == cur {
                break;
            }
            match nodes.kind(parent_id) {
                AstKind::TSAsExpression(_) => return true,
                AstKind::ParenthesizedExpression(_) => {
                    cur = parent_id;
                }
                _ => break,
            }
        }
    }
    false
}

/// True when `annotation` is a `x is T` type predicate (`TSTypePredicate`).
/// Such a return type narrows per call site and cannot collapse into a plain
/// union without erasing the narrowing.
#[must_use]
pub fn type_annotation_is_type_predicate(
    annotation: Option<&oxc_ast::ast::TSTypeAnnotation>,
) -> bool {
    use oxc_ast::ast::TSType;
    annotation.is_some_and(|ann| matches!(ann.type_annotation, TSType::TSTypePredicate(_)))
}

/// True when `annotation` is a return-type that admits both `return;` (yields
/// `undefined`) and `return expr;`: a bare `void`/`undefined` keyword, or a union
/// that includes either. Mixing bare and value returns under such a contract is
/// the canonical TypeScript idiom (e.g. `: T | undefined`, void tail-calls), not
/// an inconsistency.
#[must_use]
pub fn return_type_admits_void_or_undefined(
    annotation: Option<&oxc_ast::ast::TSTypeAnnotation>,
) -> bool {
    use oxc_ast::ast::TSType;
    fn is_void_or_undefined(ty: &TSType) -> bool {
        matches!(ty, TSType::TSVoidKeyword(_) | TSType::TSUndefinedKeyword(_))
    }
    annotation.is_some_and(|ann| match &ann.type_annotation {
        TSType::TSUnionType(union) => union.types.iter().any(is_void_or_undefined),
        ty => is_void_or_undefined(ty),
    })
}

/// True when any enclosing function/arrow of `node_id` declares a type-predicate
/// (`value is T`) return type. Such a function IS a hand-written type guard: the
/// `as` casts in its body are needed to read discriminant properties off the
/// loosely-typed input before returning the narrowing, so flagging them is
/// circular (every alternative guard would need the same cast).
#[must_use]
pub fn is_inside_type_predicate_fn(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node_id) {
        let return_type = match ancestor.kind() {
            AstKind::Function(f) => f.return_type.as_deref(),
            AstKind::ArrowFunctionExpression(a) => a.return_type.as_deref(),
            _ => None,
        };
        if type_annotation_is_type_predicate(return_type) {
            return true;
        }
    }
    false
}

/// True when `kind` is a type-only binding context — a node whose name lives
/// purely in the type namespace and is erased at runtime (function/constructor
/// types, call/construct/method/index signatures, mapped types, `infer`, plus
/// `type` aliases and interfaces). A value binding sharing such a name shadows
/// nothing observable.
#[must_use]
pub fn is_type_only_binding_context(kind: oxc_ast::AstKind<'_>) -> bool {
    use oxc_ast::AstKind;
    matches!(
        kind,
        AstKind::TSFunctionType(_)
            | AstKind::TSConstructorType(_)
            | AstKind::TSCallSignatureDeclaration(_)
            | AstKind::TSConstructSignatureDeclaration(_)
            | AstKind::TSMethodSignature(_)
            | AstKind::TSIndexSignature(_)
            | AstKind::TSMappedType(_)
            | AstKind::TSInferType(_)
            | AstKind::TSTypeAliasDeclaration(_)
            | AstKind::TSInterfaceDeclaration(_)
    )
}

/// True when `decl_node` declares a binding from a type-only import — either a
/// whole `import type ...` declaration or an individual `import { type X }`
/// specifier. These exist only in the type namespace and are erased at runtime,
/// so a value binding of the same name does not shadow them.
#[must_use]
pub fn is_type_only_import_binding(
    nodes: &oxc_semantic::AstNodes<'_>,
    decl_node: oxc_semantic::NodeId,
) -> bool {
    use oxc_ast::AstKind;
    std::iter::once(nodes.kind(decl_node))
        .chain(nodes.ancestor_kinds(decl_node))
        .any(|kind| match kind {
            AstKind::ImportDeclaration(import) => import.import_kind.is_type(),
            AstKind::ImportSpecifier(spec) => spec.import_kind.is_type(),
            _ => false,
        })
}

/// Known database / ORM / query-builder packages. A file that imports none of
/// these does not talk to a database, so database-specific rules
/// (e.g. `db-no-n-plus-one`) must not fire on it.
///
/// Matched against the *root* package of every import specifier in the file
/// (`drizzle-orm/node-postgres` → `drizzle-orm`), so subpath imports count.
const DB_PACKAGES: &[&str] = &[
    "drizzle-orm",
    "@prisma/client",
    "prisma",
    "typeorm",
    "@mikro-orm/core",
    "sequelize",
    "knex",
    "mongoose",
    "mongodb",
    "pg",
    "postgres",
    "mysql",
    "mysql2",
    "sqlite",
    "sqlite3",
    "better-sqlite3",
    "@planetscale/database",
    "@neondatabase/serverless",
    "kysely",
    "objection",
    "bookshelf",
    "ioredis",
    "redis",
];

/// Root package of a bare import specifier: `@scope/pkg/deep` → `@scope/pkg`,
/// `drizzle-orm/node-postgres` → `drizzle-orm`. Relative specifiers (`./db`) are
/// returned unchanged and never match a package name.
fn import_root_package(specifier: &str) -> &str {
    if specifier.starts_with('@') {
        let end = specifier
            .match_indices('/')
            .nth(1)
            .map(|(idx, _)| idx)
            .unwrap_or(specifier.len());
        return &specifier[..end];
    }
    specifier.split('/').next().unwrap_or(specifier)
}

/// True when the file imports at least one known database / ORM package
/// ([`DB_PACKAGES`]). Covers static `import`/`export … from`, dynamic
/// `import('…')`, and CommonJS `require('…')`.
///
/// Database rules gate on this so they never fire on files doing unrelated
/// async I/O (blob storage, HTTP, filesystem) that merely *looks* like a query.
#[must_use]
pub fn file_imports_db_library(semantic: &oxc_semantic::Semantic<'_>) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Argument, Expression};

    let is_db_specifier = |spec: &str| DB_PACKAGES.contains(&import_root_package(spec));

    semantic.nodes().iter().any(|node| match node.kind() {
        AstKind::ImportDeclaration(decl) => is_db_specifier(decl.source.value.as_str()),
        AstKind::ExportNamedDeclaration(decl) => decl
            .source
            .as_ref()
            .is_some_and(|src| is_db_specifier(src.value.as_str())),
        AstKind::ExportAllDeclaration(decl) => is_db_specifier(decl.source.value.as_str()),
        AstKind::ImportExpression(expr) => {
            matches!(peel_parens(&expr.source), Expression::StringLiteral(lit)
                if is_db_specifier(lit.value.as_str()))
        }
        AstKind::CallExpression(call) => {
            let is_require = matches!(&call.callee, Expression::Identifier(id) if id.name == "require");
            is_require
                && matches!(call.arguments.first(), Some(Argument::StringLiteral(lit))
                    if is_db_specifier(lit.value.as_str()))
        }
        _ => false,
    })
}

/// Leftmost identifier of a member/call chain: the object the chain is rooted
/// on. `x.tags.find(...)` → `"x"`, `conn.manager.qb().execute()` → `"conn"`.
/// Returns `None` for chains not rooted on a plain identifier (e.g. `this`,
/// a literal, or a parenthesised expression).
#[must_use]
pub fn receiver_root_identifier(expr: &oxc_ast::ast::Expression) -> Option<String> {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(mem) => receiver_root_identifier(&mem.object),
        Expression::ComputedMemberExpression(mem) => receiver_root_identifier(&mem.object),
        Expression::CallExpression(call) => receiver_root_identifier(&call.callee),
        _ => None,
    }
}

/// Name of the first parameter of a call's callback argument, when that
/// argument is an arrow or function expression with a plain identifier first
/// parameter: `items.map((x) => …)` → `"x"`. This is the iteration binding for
/// array-iteration callbacks (`map`/`forEach`/…). Returns `None` for spreads,
/// non-function args, or destructured/rest first parameters.
#[must_use]
pub fn callback_first_param_name(call: &oxc_ast::ast::CallExpression) -> Option<String> {
    use oxc_ast::ast::{BindingPattern, Expression};
    let expr = call.arguments.first()?.as_expression()?;
    let params = match expr {
        Expression::ArrowFunctionExpression(arrow) => &arrow.params,
        Expression::FunctionExpression(func) => &func.params,
        _ => return None,
    };
    let first_param = params.items.first()?;
    let BindingPattern::BindingIdentifier(id) = &first_param.pattern else {
        return None;
    };
    Some(id.name.to_string())
}

/// True when the chain rooted at `call` is the initializer of a `let`/`var`
/// declaration whose binding is later reassigned with a `.where(...)` member
/// call applied to the same variable (`let query = db.delete(t); … query =
/// query.where(f);`).
///
/// The builder pattern assembles a query imperatively: the `.where(...)` clause
/// is applied through a reassignment of the stored variable rather than chained
/// onto the original call, so a static chain walk never sees it. The query is
/// still guarded, so `enforce-delete-with-where` / `enforce-update-with-where`
/// gate on the absence of such a reassignment before reporting.
///
/// The binding is resolved by symbol, so a `.where(...)` reassignment of a
/// shadowing inner binding that happens to share the name does not count. A
/// reassignment that never applies `.where(...)` (e.g. `query =
/// query.returning(cols)`) does not suppress.
#[must_use]
pub fn where_applied_via_variable_reassignment(
    call: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{BindingPattern, VariableDeclarationKind};
    use oxc_semantic::ReferenceFlags;

    // The call must be the initializer of a `let`/`var` binding identifier.
    // Walk outward to the nearest declarator and resolve its symbol.
    let mut symbol_id = None;
    for ancestor in semantic.nodes().ancestors(call.id()) {
        match ancestor.kind() {
            AstKind::VariableDeclaration(decl) => {
                if !matches!(
                    decl.kind,
                    VariableDeclarationKind::Let | VariableDeclarationKind::Var
                ) || decl.declarations.len() != 1
                {
                    return false;
                }
                let BindingPattern::BindingIdentifier(id) = &decl.declarations[0].id else {
                    return false;
                };
                symbol_id = id.symbol_id.get();
                break;
            }
            // Stop at any scope or statement boundary above the call: a chain
            // not directly initialising a declarator is not a stored query.
            AstKind::BlockStatement(_)
            | AstKind::FunctionBody(_)
            | AstKind::Program(_)
            | AstKind::ExpressionStatement(_) => return false,
            _ => {}
        }
    }
    let Some(symbol_id) = symbol_id else {
        return false;
    };

    // Look at every write reference to this exact binding for a reassignment
    // whose right-hand side applies `.where(...)`.
    semantic.symbol_references(symbol_id).any(|reference| {
        if !reference.flags().contains(ReferenceFlags::Write) {
            return false;
        }
        for kind in semantic.nodes().ancestor_kinds(reference.node_id()) {
            if let AstKind::AssignmentExpression(assign) = kind {
                return chain_calls_where(&assign.right);
            }
        }
        false
    })
}

/// True when the call chain in `expr` contains a `.where(...)` member call,
/// looking through `as`/`satisfies`/`!`/parenthesis wrappers and earlier links
/// (`query.where(f).returning(c)` counts).
fn chain_calls_where(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::TSAsExpression(e) => chain_calls_where(&e.expression),
        Expression::TSSatisfiesExpression(e) => chain_calls_where(&e.expression),
        Expression::TSNonNullExpression(e) => chain_calls_where(&e.expression),
        Expression::ParenthesizedExpression(e) => chain_calls_where(&e.expression),
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                if member.property.name.as_str() == "where" {
                    return true;
                }
                return chain_calls_where(&member.object);
            }
            false
        }
        Expression::StaticMemberExpression(member) => chain_calls_where(&member.object),
        _ => false,
    }
}

/// Packages whose `useForm` export is React Hook Form's. `react-hook-form`
/// itself plus the `@hookform/*` resolver/devtools scope, which re-export it.
fn is_react_hook_form_package(specifier: &str) -> bool {
    let root = import_root_package(specifier);
    root == "react-hook-form" || root.starts_with("@hookform/")
}

/// True when the local binding `local_name` (e.g. `useForm`) is imported from a
/// package other than React Hook Form — most notably `@tanstack/react-form`,
/// whose `useForm` shares the name but has a different API.
///
/// React-Hook-Form rules gate on this so they never fire on a same-named hook
/// from another library. A file that imports `useForm` from React Hook Form, or
/// uses a `useForm` it never imports, is *not* exempt: the binding is absent or
/// genuinely React Hook Form's.
#[must_use]
pub fn local_binding_imported_from_foreign_package(
    semantic: &oxc_semantic::Semantic<'_>,
    local_name: &str,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::ImportDeclarationSpecifier;

    semantic.nodes().iter().any(|node| {
        let AstKind::ImportDeclaration(decl) = node.kind() else {
            return false;
        };
        if is_react_hook_form_package(decl.source.value.as_str()) {
            return false;
        }
        let Some(specifiers) = &decl.specifiers else {
            return false;
        };
        specifiers.iter().any(|spec| {
            let local = match spec {
                ImportDeclarationSpecifier::ImportSpecifier(named) => &named.local,
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => &def.local,
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => &ns.local,
            };
            local.name.as_str() == local_name
        })
    })
}

#[cfg(test)]
mod oxc_helpers_tests {
    use super::{byte_offset_to_line_col, mask_comments, reset_file_caches, source_contains};

    /// Reference O(byte_offset) scan that `byte_offset_to_line_col` must agree
    /// with for every offset.
    fn naive_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for (i, c) in source.char_indices() {
            if i >= byte_offset {
                break;
            }
            if c == '\r' {
                continue;
            }
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    #[test]
    fn matches_std_for_hits_and_misses_and_caches() {
        reset_file_caches();
        let src = "import React from 'react';\nconst x = items.find(p);";
        // First call computes, second is served from the memo — both must agree
        // with std::str::contains.
        assert_eq!(source_contains(src, "react"), src.contains("react"));
        assert!(source_contains(src, "react"));
        assert_eq!(source_contains(src, "angular"), src.contains("angular"));
        assert!(!source_contains(src, "angular"));
    }

    #[test]
    fn invalidates_when_source_identity_changes() {
        reset_file_caches();
        let a = String::from("has a react import");
        assert!(source_contains(&a, "react"));
        // A distinct source (different ptr) must never return `a`'s cached hit.
        let b = String::from("no framework here at all");
        assert_eq!(source_contains(&b, "react"), b.contains("react"));
        assert!(!source_contains(&b, "react"));
    }

    #[test]
    fn reset_then_recompute_stays_correct() {
        reset_file_caches();
        let s = "needle in a haystack";
        assert!(source_contains(s, "needle"));
        reset_file_caches();
        assert_eq!(source_contains(s, "needle"), s.contains("needle"));
        assert_eq!(source_contains(s, "missing"), s.contains("missing"));
    }

    #[test]
    fn byte_offset_matches_naive_scan_with_crlf_and_utf8() {
        reset_file_caches();
        // LF, CRLF, a 2-byte char (é), spaces — every char-boundary offset
        // must match the reference scan.
        let src = "ab\nc\r\ndé f\nghi";
        for off in 0..=src.len() {
            if !src.is_char_boundary(off) {
                continue;
            }
            assert_eq!(
                byte_offset_to_line_col(src, off),
                naive_line_col(src, off),
                "mismatch at byte offset {off}"
            );
        }
    }

    #[test]
    fn byte_offset_rebuilds_index_on_source_change() {
        reset_file_caches();
        let a = String::from("one\ntwo\nthree");
        assert_eq!(byte_offset_to_line_col(&a, 4), (2, 1)); // start of "two"
        // Different source, no explicit reset — the index must rebuild itself.
        let b = String::from("x\ny\nz\nw");
        assert_eq!(byte_offset_to_line_col(&b, 6), (4, 1)); // start of "w"
    }

    use super::{
        ClassShape, expression_is_or_resolves_to_literal, file_imports_db_library,
        has_ts_expect_error_above, is_as_unknown_double_cast, is_outer_as_unknown_double_cast,
        node_has_preceding_deprecated_tag, peel_parens, type_annotation_is_type_predicate,
        with_semantic,
    };
    use oxc_ast::AstKind;
    use oxc_span::SourceType;

    fn imports_db(src: &str) -> bool {
        with_semantic(src, SourceType::ts(), file_imports_db_library)
    }

    #[test]
    fn file_imports_db_library_detects_static_import_and_subpath() {
        assert!(imports_db("import { drizzle } from 'drizzle-orm/node-postgres';"));
        assert!(imports_db("import { PrismaClient } from '@prisma/client';"));
        assert!(imports_db("import postgres from 'postgres';"));
    }

    #[test]
    fn file_imports_db_library_detects_require_and_dynamic_import() {
        assert!(imports_db("const pg = require('pg');"));
        assert!(imports_db("const m = await import('mongodb');"));
        assert!(imports_db("export * from 'knex';"));
    }

    #[test]
    fn file_imports_db_library_rejects_non_db_imports() {
        assert!(!imports_db("import { ContainerClient } from '@azure/storage-blob';"));
        assert!(!imports_db("import fs from 'node:fs';"));
        assert!(!imports_db("import { foo } from './db';"));
        assert!(!imports_db("const x = 1;"));
    }

    #[test]
    fn class_shape_separates_decorator_super_and_implements() {
        // Each axis is reported independently so callers exempt on exactly the
        // axis they care about (no bundled "has_heritage").
        with_semantic("class A {}", SourceType::ts(), |sem| {
            let class = find_class(sem);
            let shape = ClassShape::of(class);
            assert!(!shape.is_decorated && !shape.has_super_class && !shape.has_implements);
        });
        with_semantic("class A extends B {}", SourceType::ts(), |sem| {
            let shape = ClassShape::of(find_class(sem));
            assert!(shape.has_super_class && !shape.has_implements && !shape.is_decorated);
        });
        with_semantic("class A implements I {}", SourceType::ts(), |sem| {
            let shape = ClassShape::of(find_class(sem));
            assert!(shape.has_implements && !shape.has_super_class && !shape.is_decorated);
        });
        with_semantic("@Dec()\nclass A {}", SourceType::ts(), |sem| {
            let shape = ClassShape::of(find_class(sem));
            assert!(shape.is_decorated && !shape.has_super_class && !shape.has_implements);
        });
    }

    #[test]
    fn peel_parens_unwraps_nested_parentheses() {
        // `((x))` peels through both parenthesized layers to the identifier `x`.
        with_semantic("const y = ((x));", SourceType::ts(), |sem| {
            let init = sem
                .nodes()
                .iter()
                .find_map(|n| match n.kind() {
                    AstKind::VariableDeclarator(d) => d.init.as_ref(),
                    _ => None,
                })
                .expect("a variable initializer");
            assert!(
                matches!(init, oxc_ast::ast::Expression::ParenthesizedExpression(_)),
                "outermost init is parenthesized"
            );
            assert!(
                matches!(peel_parens(init), oxc_ast::ast::Expression::Identifier(id) if id.name == "x"),
                "peeling reaches the bare identifier"
            );
        });
    }

    #[test]
    fn double_cast_helpers_match_both_rule_semantics() {
        // Outer half: the outer `as T` of `x as unknown as T`.
        // - is_outer_as_unknown_double_cast (ts-no-as-narrowing) → true
        // - is_as_unknown_double_cast (no-type-assertion) → also true
        with_semantic(
            "declare const x: unknown; const y = x as unknown as Foo;",
            SourceType::ts(),
            |sem| {
                let (node_id, outer) = as_expr_with_target(sem, "Foo");
                assert!(
                    is_outer_as_unknown_double_cast(outer),
                    "outer half is the canonical escape hatch"
                );
                assert!(
                    is_as_unknown_double_cast(node_id, outer, sem),
                    "no-type-assertion exempts the outer half too"
                );
            },
        );

        // A double cast WITHOUT an `unknown` middle (`x as any as Foo`) is not
        // the escape hatch — neither helper exempts the outer cast.
        with_semantic(
            "declare const x: unknown; const y = x as any as Foo;",
            SourceType::ts(),
            |sem| {
                let (node_id, outer) = as_expr_with_target(sem, "Foo");
                assert!(!is_outer_as_unknown_double_cast(outer));
                assert!(!is_as_unknown_double_cast(node_id, outer, sem));
            },
        );

        // Inner half: the `as unknown` of `x as unknown as Foo`. Only
        // no-type-assertion (is_as_unknown_double_cast) exempts this half;
        // is_outer_as_unknown_double_cast must NOT, since its inner is `x`.
        with_semantic(
            "declare const x: unknown; const y = x as unknown as Foo;",
            SourceType::ts(),
            |sem| {
                let (node_id, inner) = as_expr_to_unknown(sem);
                assert!(
                    !is_outer_as_unknown_double_cast(inner),
                    "ts-no-as-narrowing does NOT exempt the inner `as unknown`"
                );
                assert!(
                    is_as_unknown_double_cast(node_id, inner, sem),
                    "no-type-assertion DOES exempt the inner `as unknown` half"
                );
            },
        );

        // Second hoisted predicate benefiting a rule with no tree-sitter twin
        // (no-redundant-null-undefined-check): a `value is T` return type is a
        // type predicate.
        with_semantic(
            "function isT(v: unknown): v is T { return true as boolean; }",
            SourceType::ts(),
            |sem| {
                let func = sem.nodes().iter().find_map(|n| match n.kind() {
                    AstKind::Function(f) => Some(f),
                    _ => None,
                });
                let f = func.expect("a function declaration");
                assert!(type_annotation_is_type_predicate(f.return_type.as_deref()));
            },
        );
    }

    /// Right operand of the first `BinaryExpression` in the program.
    fn binary_right<'a>(
        sem: &'a oxc_semantic::Semantic<'a>,
    ) -> &'a oxc_ast::ast::Expression<'a> {
        sem.nodes()
            .iter()
            .find_map(|n| match n.kind() {
                AstKind::BinaryExpression(b) => Some(&b.right),
                _ => None,
            })
            .expect("a binary expression")
    }

    #[test]
    fn expression_is_or_resolves_to_literal_inline_and_const_bound() {
        // Inline primitive literals.
        with_semantic("x === \"abc\";", SourceType::ts(), |sem| {
            assert!(expression_is_or_resolves_to_literal(binary_right(sem), sem));
        });
        with_semantic("x === 42;", SourceType::ts(), |sem| {
            assert!(expression_is_or_resolves_to_literal(binary_right(sem), sem));
        });
        // Const bound to a string literal (the ethers.js shape).
        with_semantic(
            "const k = \"9f7d\"; function f(a) { return a === k; }",
            SourceType::ts(),
            |sem| assert!(expression_is_or_resolves_to_literal(binary_right(sem), sem)),
        );
        // Const bound to a numeric literal.
        with_semantic(
            "const k = 12345; function f(a) { if (a === k) {} }",
            SourceType::ts(),
            |sem| assert!(expression_is_or_resolves_to_literal(binary_right(sem), sem)),
        );
    }

    #[test]
    fn expression_is_or_resolves_to_literal_rejects_non_literals() {
        // Bound to a call → not a literal.
        with_semantic(
            "const k = getSecret(); function f(a) { if (a === k) {} }",
            SourceType::ts(),
            |sem| assert!(!expression_is_or_resolves_to_literal(binary_right(sem), sem)),
        );
        // Bound to a member access → not a literal.
        with_semantic(
            "const k = process.env.KEY; function f(a) { if (a === k) {} }",
            SourceType::ts(),
            |sem| assert!(!expression_is_or_resolves_to_literal(binary_right(sem), sem)),
        );
        // A literal nested inside a larger expression → not a direct literal.
        with_semantic(
            "const k = \"a\" + salt; function f(a) { if (a === k) {} }",
            SourceType::ts(),
            |sem| assert!(!expression_is_or_resolves_to_literal(binary_right(sem), sem)),
        );
        // An unresolved free identifier → not a literal.
        with_semantic("x === secret;", SourceType::ts(), |sem| {
            assert!(!expression_is_or_resolves_to_literal(binary_right(sem), sem));
        });
    }

    /// First `Class` node in the program.
    fn find_class<'a>(sem: &'a oxc_semantic::Semantic<'a>) -> &'a oxc_ast::ast::Class<'a> {
        sem.nodes()
            .iter()
            .find_map(|n| match n.kind() {
                AstKind::Class(c) => Some(c),
                _ => None,
            })
            .expect("a class declaration")
    }

    /// The `TSAsExpression` whose target type is the identifier `target`, with
    /// its `NodeId`.
    fn as_expr_with_target<'a>(
        sem: &'a oxc_semantic::Semantic<'a>,
        target: &str,
    ) -> (oxc_semantic::NodeId, &'a oxc_ast::ast::TSAsExpression<'a>) {
        use oxc_ast::ast::{TSType, TSTypeName};
        sem.nodes()
            .iter()
            .find_map(|n| match n.kind() {
                AstKind::TSAsExpression(a) => match &a.type_annotation {
                    TSType::TSTypeReference(r) => match &r.type_name {
                        TSTypeName::IdentifierReference(id) if id.name.as_str() == target => {
                            Some((n.id(), a))
                        }
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            })
            .expect("a TSAsExpression with the target type")
    }

    /// The `TSAsExpression` whose target type is the `unknown` keyword, with its
    /// `NodeId`.
    fn as_expr_to_unknown<'a>(
        sem: &'a oxc_semantic::Semantic<'a>,
    ) -> (oxc_semantic::NodeId, &'a oxc_ast::ast::TSAsExpression<'a>) {
        use oxc_ast::ast::TSType;
        sem.nodes()
            .iter()
            .find_map(|n| match n.kind() {
                AstKind::TSAsExpression(a)
                    if matches!(a.type_annotation, TSType::TSUnknownKeyword(_)) =>
                {
                    Some((n.id(), a))
                }
                _ => None,
            })
            .expect("an `as unknown` expression")
    }

    #[test]
    fn mask_comments_blanks_line_comment_keeps_code() {
        let masked = mask_comments("let x = 1; // findMany(\nlet y = 2;");
        assert!(!masked.contains("findMany("));
        assert!(masked.contains("let x = 1;"));
        assert!(masked.contains("let y = 2;"));
    }

    #[test]
    fn mask_comments_blanks_jsdoc_block() {
        let src = "/**\n * @example\n * prisma.user.findMany()\n */\nconst a = 1;";
        let masked = mask_comments(src);
        assert!(!masked.contains("findMany"));
        assert!(masked.contains("const a = 1;"));
    }

    #[test]
    fn mask_comments_preserves_length_and_newlines() {
        let src = "/* a */\n// b\nlet z = 0;";
        let masked = mask_comments(src);
        assert_eq!(masked.len(), src.len());
        assert_eq!(masked.matches('\n').count(), src.matches('\n').count());
    }

    #[test]
    fn mask_comments_ignores_comment_markers_inside_strings() {
        let src = r#"const url = "https://example.com/path"; const c = 1;"#;
        let masked = mask_comments(src);
        assert_eq!(masked, src);
    }

    #[test]
    fn mask_comments_handles_multibyte_inside_comment() {
        let src = "let x = 1; // café ☕\nlet y = 2;";
        let masked = mask_comments(src);
        assert_eq!(masked.len(), src.len());
        assert!(masked.contains("let y = 2;"));
        assert!(!masked.contains("café"));
    }

    /// Byte offset of the `type Dup` declaration in `src`.
    fn type_dup_start(src: &str) -> usize {
        src.find("type Dup").expect("a `type Dup` declaration")
    }

    #[test]
    fn ts_expect_error_above_detects_line_and_block_forms() {
        for src in [
            "// @ts-expect-error\ntype Dup = X;",
            "/* @ts-expect-error */\ntype Dup = X;",
        ] {
            with_semantic(src, SourceType::ts(), |sem| {
                assert!(
                    has_ts_expect_error_above(sem.comments(), src, type_dup_start(src)),
                    "directive directly above should match: {src:?}"
                );
            });
        }
    }

    #[test]
    fn ts_expect_error_above_rejects_when_code_intervenes() {
        // A non-blank line between the directive and the declaration breaks the
        // adjacency — the directive applies to `type Other`, not `type Dup`.
        let src = "// @ts-expect-error\ntype Other = X;\ntype Dup = Y;";
        with_semantic(src, SourceType::ts(), |sem| {
            assert!(!has_ts_expect_error_above(sem.comments(), src, type_dup_start(src)));
        });
    }

    #[test]
    fn ts_expect_error_above_rejects_plain_comment() {
        let src = "// just a note\ntype Dup = X;";
        with_semantic(src, SourceType::ts(), |sem| {
            assert!(!has_ts_expect_error_above(sem.comments(), src, type_dup_start(src)));
        });
    }

    /// Byte offset of the `foo` method name in `src`.
    fn foo_method_start(src: &str) -> usize {
        src.find("foo").expect("a `foo` method")
    }

    #[test]
    fn deprecated_tag_above_detects_jsdoc_and_line_forms() {
        for src in [
            "class A { /** @deprecated use bar instead */ foo() {} }",
            "class A {\n  // @deprecated\n  foo() {}\n}",
        ] {
            with_semantic(src, SourceType::ts(), |sem| {
                assert!(
                    node_has_preceding_deprecated_tag(sem.comments(), src, foo_method_start(src)),
                    "tag directly above should match: {src:?}"
                );
            });
        }
    }

    #[test]
    fn deprecated_tag_above_rejects_when_another_member_intervenes() {
        // The tag is the leading comment of `other`, not of `foo` below it.
        let src = "class A { /** @deprecated */ other() {} foo() {} }";
        with_semantic(src, SourceType::ts(), |sem| {
            assert!(!node_has_preceding_deprecated_tag(sem.comments(), src, foo_method_start(src)));
        });
    }

    #[test]
    fn deprecated_tag_above_rejects_plain_comment() {
        let src = "class A { /** does a thing */ foo() {} }";
        with_semantic(src, SourceType::ts(), |sem| {
            assert!(!node_has_preceding_deprecated_tag(sem.comments(), src, foo_method_start(src)));
        });
    }

    fn destructured_keys(source: &str) -> Vec<String> {
        let mut keys: Vec<String> = super::groups_destructure_keys(source).into_iter().collect();
        keys.sort();
        keys
    }

    #[test]
    fn groups_destructure_collects_shorthand_keys() {
        assert_eq!(
            destructured_keys("const {code, openingFence, indent} = match.groups ?? {};"),
            vec!["code", "indent", "openingFence"]
        );
    }

    #[test]
    fn groups_destructure_collects_from_exec_optional_chain() {
        assert_eq!(
            destructured_keys("const { year } = /(?<year>\\d{4})/.exec(s)?.groups ?? {};"),
            vec!["year"]
        );
    }

    #[test]
    fn groups_destructure_collects_renamed_key_not_binding() {
        assert_eq!(destructured_keys("const { year: y } = m.groups;"), vec!["year"]);
    }

    #[test]
    fn groups_destructure_collects_optional_chained() {
        assert_eq!(destructured_keys("const { a, b } = m?.groups;"), vec!["a", "b"]);
    }

    #[test]
    fn groups_destructure_skips_rest_property() {
        assert_eq!(destructured_keys("const { a, ...rest } = m.groups;"), vec!["a"]);
    }

    #[test]
    fn groups_destructure_ignores_non_groups_source() {
        assert!(destructured_keys("const { a, b } = obj.fields;").is_empty());
    }

    #[test]
    fn groups_destructure_ignores_groups_prefix_identifier() {
        assert!(destructured_keys("const { a } = obj.groupsCount;").is_empty());
    }

    #[test]
    fn groups_destructure_ignores_direct_property_access() {
        assert!(destructured_keys("const x = match.groups.year;").is_empty());
    }
}
