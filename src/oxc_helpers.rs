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

/// Collect the plain binding identifiers that hold an entire `<expr>.groups`
/// object, e.g. `m` from `const m = input.match(re)?.groups || {}`. Here the
/// `.groups` object is stored in a variable — neither destructured nor spread —
/// and its named-group properties are read later as `m.name`; callers pair each
/// returned binding with the regex's group names and check for those reads via
/// [`reads_var_property`].
///
/// Mirrors [`groups_destructure_keys`]'s byte scan: every `.groups` member
/// access is located, then — for accesses that are the terminal initializer of
/// a `const`/`let`/`var` declaration bound to a bare identifier — the identifier
/// is captured. A `.groups` that is sub-accessed (`.groups.name`,
/// `.groups["name"]`), destructured (`{ name } = …groups`), or not bound to a
/// fresh identifier is skipped. Optional chaining (`?.groups`) and a trailing
/// `|| {}` / `?? {}` fallback are tolerated since they sit outside the binding
/// identifier.
#[must_use]
pub fn groups_object_binding_names(source: &str) -> FxHashSet<String> {
    let mut names = FxHashSet::default();
    let bytes = source.as_bytes();
    let needle = b".groups";
    let mut search_from = 0;
    while let Some(rel) = memchr::memmem::find(&bytes[search_from..], needle) {
        let dot = search_from + rel;
        let after = dot + needle.len();
        search_from = after;
        // `.groups` must be a member access (not a prefix like `.groupsCount`)
        // and the terminal object, not a sub-access (`.groups.name`,
        // `.groups["name"]`) whose binding would hold a single group value.
        if bytes
            .get(after)
            .is_some_and(|&b| is_ident_byte(b) || b == b'.' || b == b'[')
        {
            continue;
        }
        if let Some((start, end)) = binding_ident_before_groups(bytes, dot) {
            names.insert(source[start..end].to_string());
        }
    }
    names
}

/// True when the file reads property `name` off the variable `var_name` as a
/// real property access — `var_name.name` or `var_name?.name` — the read used
/// after the entire `.groups` object was stored in `var_name`. The match
/// respects identifier boundaries: `var_name` must not be the tail of a longer
/// identifier or itself a property (`param.name`, `obj.m.name` never match an
/// `m` binding), and `name` must not be the head of a longer identifier
/// (`m.repository` does not satisfy group `repo`).
#[must_use]
pub fn reads_var_property(source: &str, var_name: &str, name: &str) -> bool {
    let bytes = source.as_bytes();
    let mut from = 0;
    while let Some(rel) = memchr::memmem::find(&bytes[from..], var_name.as_bytes()) {
        let start = from + rel;
        let end = start + var_name.len();
        from = start + 1;
        // Left boundary: `var_name` must start a fresh identifier, not continue
        // one (`param`) and not be a property of another object (`obj.m`).
        if start > 0 && (is_ident_byte(bytes[start - 1]) || bytes[start - 1] == b'.') {
            continue;
        }
        // `var_name.name` or `var_name?.name`.
        let mut j = end;
        if bytes.get(j) == Some(&b'?') {
            j += 1;
        }
        if bytes.get(j) != Some(&b'.') {
            continue;
        }
        j += 1;
        if !bytes[j..].starts_with(name.as_bytes()) {
            continue;
        }
        // Right boundary: `name` must end the property, not prefix a longer one.
        if bytes.get(j + name.len()).is_some_and(|&b| is_ident_byte(b)) {
            continue;
        }
        return true;
    }
    false
}

/// True when the file spreads an entire `<expr>.groups` object anywhere, e.g.
/// `{ ...match.groups, code: match[0] }` (the unjs/mlly `matchAll` pattern). A
/// spread copies every key of the `.groups` object, so each named capturing
/// group flows out as a property with no individual `.groups.name` read or
/// `{ name } = .groups` destructure. The specific group names are unknowable
/// from a spread, so — consistent with [`groups_destructure_keys`]'s
/// conservative, file-level stance — a single such spread marks every named
/// group in the file as referenced.
///
/// Matches the AST shape of a `SpreadElement` (object `{ ...x.groups }`, array
/// `[ ...x.groups ]`, or call `f(...x.groups)`) whose argument is a member
/// access whose final property is `groups` (`...match.groups`, `...m?.groups`).
/// A direct `match.groups.year` read or a `{ name } = m.groups` destructure is
/// not a `SpreadElement` argument, so neither matches here.
#[must_use]
pub fn file_has_groups_spread(semantic: &Semantic) -> bool {
    use oxc_ast::AstKind;
    semantic.nodes().iter().any(|node| {
        let AstKind::SpreadElement(spread) = node.kind() else {
            return false;
        };
        expression_ends_in_groups_member(&spread.argument)
    })
}

/// True when `expr` is a member access whose final property is `groups`
/// (`x.groups`, `re.exec(s).groups`, `m?.groups`). Unwraps a leading optional
/// `ChainExpression` so `...m?.groups` is recognised alongside `...m.groups`.
fn expression_ends_in_groups_member(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::{ChainElement, Expression};
    match expr {
        Expression::StaticMemberExpression(member) => member.property.name.as_str() == "groups",
        Expression::ChainExpression(chain) => matches!(
            &chain.expression,
            ChainElement::StaticMemberExpression(member)
                if member.property.name.as_str() == "groups"
        ),
        _ => false,
    }
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

/// Given the byte offset of the `.` in a terminal `<expr>.groups` access, return
/// the byte range of the plain binding identifier it initializes in a
/// `const`/`let`/`var` declaration — `m` in `const m = <expr>.groups`. Returns
/// `None` when the access is not the initializer of such a binding: destructured
/// (`{…} = …groups`, the `}` to the left of `=`), compared (`a === b.groups`),
/// returned, passed as an argument, or assigned to a member target
/// (`obj.m = …groups`).
///
/// Mirrors [`object_pattern_before_groups`]'s left walk over `<expr>` to the
/// top-level `=`, but extracts the identifier to the left of `=` instead of an
/// object pattern.
fn binding_ident_before_groups(bytes: &[u8], groups_dot: usize) -> Option<(usize, usize)> {
    let mut i = groups_dot;
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
                // The binding identifier sits to the left of `=`, skipping ws.
                let mut end = i;
                while end > 0 && bytes[end - 1].is_ascii_whitespace() {
                    end -= 1;
                }
                let mut start = end;
                while start > 0 && is_ident_byte(bytes[start - 1]) {
                    start -= 1;
                }
                if start == end {
                    return None;
                }
                return binding_is_declared(bytes, start).then_some((start, end));
            }
            _ => {}
        }
    }
    None
}

/// True when the identifier starting at `ident_start` is a fresh binding,
/// introduced by a `const`/`let`/`var` keyword or a `,` continuing a
/// multi-declarator (`const a = …, m = …groups`). This excludes member targets
/// (`obj.m = …`) and bare reassignments, restricting [`binding_ident_before_groups`]
/// to declarations whose initializer is the stored `.groups` object.
fn binding_is_declared(bytes: &[u8], ident_start: usize) -> bool {
    let mut j = ident_start;
    while j > 0 && bytes[j - 1].is_ascii_whitespace() {
        j -= 1;
    }
    if j == 0 {
        return false;
    }
    if bytes[j - 1] == b',' {
        return true;
    }
    let word_end = j;
    let mut word_start = j;
    while word_start > 0 && is_ident_byte(bytes[word_start - 1]) {
        word_start -= 1;
    }
    matches!(&bytes[word_start..word_end], b"const" | b"let" | b"var")
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

/// True if the file imports from `preact/compat`, Preact's React-compatibility
/// layer — the canonical `preact/compat` entry or any subpath (`preact/compat/server`).
/// Unlike plain Preact, `preact/compat` re-implements the React API surface —
/// including `className`/`htmlFor` — so a file importing it uses the
/// React-compatible conventions, not Preact's native HTML attributes. ESM
/// `import ... from` or CommonJS `require(...)`. Memoized via [`source_contains`].
#[must_use]
pub fn imports_preact_compat(source: &str) -> bool {
    source_contains(source, "from \"preact/compat")
        || source_contains(source, "from 'preact/compat")
        || source_contains(source, "require(\"preact/compat")
        || source_contains(source, "require('preact/compat")
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

/// True if the file imports anything from Vue: `vue`, a `vue/*` subpath, or the
/// `@vue/*` scope (`@vue/runtime-core`, `@vue/composition-api`) — ESM
/// `import ... from` or CommonJS `require(...)`. React-specific rules use this to
/// exclude Vue files, whose JSX transform treats `v-model:*` / `v-on:*` / `v-bind:*`
/// namespaced attributes as first-class directives rather than React XML
/// namespaces. Memoized per file via [`source_contains`].
#[must_use]
pub fn imports_vue(source: &str) -> bool {
    source_contains(source, "from \"vue\"")
        || source_contains(source, "from 'vue'")
        || source_contains(source, "from \"vue/")
        || source_contains(source, "from 'vue/")
        || source_contains(source, "from \"@vue/")
        || source_contains(source, "from '@vue/")
        || source_contains(source, "require(\"vue\")")
        || source_contains(source, "require('vue')")
        || source_contains(source, "require(\"vue/")
        || source_contains(source, "require('vue/")
        || source_contains(source, "require(\"@vue/")
        || source_contains(source, "require('@vue/")
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
/// Preact, Qwik, Stencil, or Voby. Detected three ways: via a framework import, via an
/// in-file `@jsxImportSource` pragma, or via the nearest `tsconfig.json`'s
/// `compilerOptions.jsxImportSource` set to a non-React runtime (which injects
/// the JSX factory project-wide, so files need no framework import).
///
/// React-specific rules (`no-unknown-property`, `react-display-name`) must not
/// fire on these files: React DevTools, Fast Refresh, and React's prop
/// conventions are all React-only concerns. Source checks are memoized per file
/// via [`source_contains`].
///
/// `preact/compat` is the exception among Preact entries: it re-implements the
/// React API (including `className`/`htmlFor`), so a file importing it is treated
/// as React-compatible, not as native-attribute JSX.
#[must_use]
pub fn is_non_react_jsx_file(source: &str, project: &crate::project::ProjectCtx, path: &Path) -> bool {
    // `preact/compat` is Preact's React-compatibility layer — `className`/`htmlFor`
    // are the supported props there, exactly as in React. It must win over the
    // `preact/` non-React check below, which would otherwise match the subpath.
    if imports_preact_compat(source) {
        return false;
    }
    // An explicit per-file non-React framework signal (import or pragma) wins
    // outright — the file's JSX is unambiguously processed by that runtime.
    if source_contains(source, "solid-js")
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
        || source_contains(source, "'voby'")
        || source_contains(source, "\"voby\"")
        || has_non_react_jsx_import_source_pragma(source)
    {
        return true;
    }
    // An explicit per-file React signal (react/react-dom import, Next.js import,
    // or a `"use client"`/`"use server"` directive) means this file's JSX is
    // React, overriding any project-level non-React `jsxImportSource` default.
    if file_has_explicit_react_signal(source) {
        return false;
    }
    // Vue 3 TSX whose Vue dependency is indirect (Vuetify components import only
    // internal `@/util` wrappers, never `'vue'`): the file still uses Vuetify's
    // unambiguous Vue-JSX conventions (`genericComponent()` + `useRender()`),
    // where `class`/`for` are the native props. This file signal has no React
    // counterpart, so it runs after — not before — the React-signal guard above.
    if uses_vue_jsx_conventions(source) {
        return true;
    }
    // A `.tsx`/`.jsx` file in a non-React framework package (nearest manifest
    // declares `vue`/`solid-js` and not `react`) with no per-file React signal is
    // that framework's JSX. The React-coexistence guard lives in the helper: a
    // package declaring `react` too keeps firing, falling through to the project
    // `jsxImportSource` default rather than blanket-skipping its React files.
    if in_non_react_framework_package(project, path) {
        return true;
    }
    // Fall back to the project-level `jsxImportSource` default.
    project.has_non_react_jsx_import_source(path)
}

/// True when the file uses Vuetify's Vue 3 JSX/TSX authoring conventions even
/// without a direct `'vue'` import — Vuetify's component sources import only
/// internal `@/util` wrappers (`genericComponent`, `useRender`) around Vue's
/// `defineComponent`, never `'vue'` itself. Such files render with native HTML
/// attribute names (`class`, `for`), so React's camelCase prop conventions do
/// not apply. Recognized by the two markers that co-occur in this pattern: a
/// `genericComponent(` factory call and a `useRender(` render callback. Memoized
/// per file via [`source_contains`].
#[must_use]
fn uses_vue_jsx_conventions(source: &str) -> bool {
    source_contains(source, "genericComponent(") || source_contains(source, "useRender(")
}

/// True when the file carries an explicit React signal: it imports from
/// `react`/`react-dom`/`react/*`, imports a Next.js package (`next`, `next/*`),
/// or declares a `"use client"`/`"use server"` directive. Such a file's JSX is
/// processed by the React runtime, so `className`/`htmlFor` are correct there.
fn file_has_explicit_react_signal(source: &str) -> bool {
    imports_react(source)
        || source_contains(source, "from \"next/")
        || source_contains(source, "from 'next/")
        || source_contains(source, "from \"next\"")
        || source_contains(source, "from 'next'")
        || source_contains(source, "\"use client\"")
        || source_contains(source, "'use client'")
        || source_contains(source, "\"use server\"")
        || source_contains(source, "'use server'")
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

/// Whether a JSX element name denotes a user-defined component rather than an
/// intrinsic host/DOM element.
///
/// The oxc parser already applies React's host-vs-component rule when it builds
/// the element name, so the variant is the single source of truth: a component
/// reference (`<Foo>`, `<_Foo>`, `<Foo.Bar>`, a non-ASCII tag) is an
/// `IdentifierReference` or `MemberExpression`, while an intrinsic host tag
/// (`<div>`, `<span>`, a hyphenated custom element like `<my-element>`) stays an
/// `Identifier`, and `<svg:rect>` a `NamespacedName`. Only components memoize on
/// prop identity (`React.memo`, `PureComponent`), so referential-equality
/// concerns apply to them alone; a host element diffs its attributes by value on
/// every render regardless.
#[must_use]
pub fn jsx_element_name_is_component(name: &oxc_ast::ast::JSXElementName) -> bool {
    use oxc_ast::ast::JSXElementName;
    matches!(
        name,
        JSXElementName::IdentifierReference(_) | JSXElementName::MemberExpression(_)
    )
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
/// import in an ESM `import ... from` or CommonJS `require(...)` context
/// (`solid-js` and its subpaths, the `@solidjs/*` scope, `solid-start`,
/// `@tanstack/solid-router`), a `@jsxImportSource solid-js` pragma, or the
/// nearest `package.json` declaring `solid-js`. Requiring the import context —
/// not a bare substring — keeps a Solid package name that appears only in a URL
/// string, comment, or data literal from marking a non-Solid file as Solid. The
/// `solid-js` / `@solidjs/` import check reuses [`imports_solid`]; all source
/// checks are memoized per file via [`source_contains`].
#[must_use]
pub fn is_solid_file(source: &str, project: &crate::project::ProjectCtx, path: &Path) -> bool {
    imports_solid(source)
        || source_contains(source, "from \"solid-start\"")
        || source_contains(source, "from 'solid-start'")
        || source_contains(source, "from \"solid-start/")
        || source_contains(source, "from 'solid-start/")
        || source_contains(source, "require(\"solid-start")
        || source_contains(source, "require('solid-start")
        || source_contains(source, "from \"@tanstack/solid-router\"")
        || source_contains(source, "from '@tanstack/solid-router'")
        || source_contains(source, "from \"@tanstack/solid-router/")
        || source_contains(source, "from '@tanstack/solid-router/")
        || source_contains(source, "require(\"@tanstack/solid-router")
        || source_contains(source, "require('@tanstack/solid-router")
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

/// True when a parameter carries an accessibility (`public` / `private` /
/// `protected`) or `readonly` modifier — a TypeScript *parameter property*.
/// Such a parameter is not a free local binding: it declares an instance field,
/// so its identifier becomes the class's property name, governed by the
/// data-model / implemented-interface contract rather than chosen freely at the
/// call site. Keyed on the OXC modifier fields (`accessibility`, `readonly`), so
/// an ordinary modifier-less parameter (`constructor(flag: boolean)`) is not a
/// parameter property.
#[must_use]
pub fn is_parameter_property(param: &oxc_ast::ast::FormalParameter) -> bool {
    param.accessibility.is_some() || param.readonly
}

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
/// whose initializer constructs a fresh local object (`is_fresh_copy_expression`):
/// an object literal / object-spread (`{ key: val }` / `{ ...other }`) or
/// `Object.assign(<fresh>, …)` / `Object.assign(Object.create(null), …)`. Such a
/// binding is a freshly-created local builder, not a reference to shared state:
/// assigning its properties (`value.x = ...`) or deleting them (`delete value.x`)
/// before returning it is the object analogue of the `const items = [];
/// items.push(x)` accumulator pattern, and mutates no external state.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, then
/// inspects the `VariableDeclarator` (whose `kind` carries the declaration
/// keyword). A function parameter, imported binding, or `this` resolves to a
/// non-`VariableDeclarator` declaration; a `var` binding or a non-fresh-copy
/// initializer is rejected, so any mutation through it is still flagged.
#[must_use]
pub fn is_local_object_builder_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::VariableDeclarationKind;

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
            ) && decl.init.as_ref().is_some_and(is_fresh_copy_expression);
        }
    }
    false
}

/// True when `expr` constructs a brand-new object that copies from existing
/// values — a fresh shallow copy whose properties can be assigned without
/// touching the source:
/// - an object literal / object-spread `{ ...x }` / `{ a: 1 }`
///   (`Expression::ObjectExpression`);
/// - `Object.assign(<fresh>, …)` where the first argument is itself a fresh
///   target — an object literal (`{}`) or `Object.create(null)`. The result
///   object is the (new) first argument, so the assignment produces a fresh
///   object rather than mutating an existing one. `Object.assign(existing, …)`,
///   whose first argument is an identifier or member expression, is NOT fresh —
///   it mutates `existing` in place and stays subject to the rule.
fn is_fresh_copy_expression<'a>(expr: &'a oxc_ast::ast::Expression<'a>) -> bool {
    use oxc_ast::ast::Expression;
    // Peel transparent wrappers first: `{} as ThemePalette` (and `satisfies T` /
    // `!` / `(…)`) evaluates to the same fresh object as the bare `{}`. See
    // `peel_value_wrappers` for the full set of value-preserving wrappers stripped.
    match peel_value_wrappers(expr) {
        Expression::ObjectExpression(_) => true,
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            if obj.name.as_str() != "Object" || member.property.name.as_str() != "assign" {
                return false;
            }
            // First argument must itself be a freshly-created target.
            match call.arguments.first().and_then(|arg| arg.as_expression()) {
                Some(Expression::ObjectExpression(_)) => true,
                Some(Expression::CallExpression(inner)) => is_object_create_null(inner),
                _ => false,
            }
        }
        _ => false,
    }
}

/// True when `call` is `Object.create(null)` — produces a fresh prototype-less
/// object, a valid fresh `Object.assign` target.
fn is_object_create_null(call: &oxc_ast::ast::CallExpression) -> bool {
    use oxc_ast::ast::Expression;
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name.as_str() == "Object"
        && member.property.name.as_str() == "create"
        && matches!(
            call.arguments.first().and_then(|arg| arg.as_expression()),
            Some(Expression::NullLiteral(_))
        )
}

/// True when the method call whose callee is the member expression `member`
/// hangs off a Prisma model-delegate accessor — its receiver (`member.object`)
/// is itself a member access (`<client>.<model>` or `<client>["<model>"]`).
///
/// Prisma delegate queries and writes are always shaped
/// `<client>.<model>.<method>(...)`, so the receiver must be a member
/// expression. This rejects a wrapper/base-service self-call whose receiver is
/// `this` or a bare identifier (`this.findMany()`, `repo.findMany()`) and a
/// factory/DI call that passes the client as an argument
/// (`loader.create(prisma)`) — in both the receiver is not a `.<model>`
/// accessor, so the call is not a delegate call.
#[must_use]
pub fn is_prisma_delegate_call(member: &oxc_ast::ast::StaticMemberExpression) -> bool {
    use oxc_ast::ast::Expression;
    matches!(
        &member.object,
        Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
    )
}

/// `true` when `expr` is an array literal (`[...]`, `[]`) or a `new Array(...)`
/// construction.
#[must_use]
pub fn is_array_initializer(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::ArrayExpression(_) => true,
        Expression::NewExpression(new_expr) => {
            matches!(&new_expr.callee, Expression::Identifier(id) if id.name.as_str() == "Array")
        }
        _ => false,
    }
}

/// Resolve `ident` to the initializer of its declaring `const`/`let` when that
/// binding lives in an inner (non-module) scope — a locally-owned binding whose
/// mutation is not observable outside the declaring function.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, then
/// returns its `VariableDeclarator` initializer. Returns `None` for a function
/// parameter, imported binding, or `this` (no `VariableDeclarator`), for a
/// module/root-scope binding, or for a declarator with no initializer — so a
/// caller judging freshness on the returned initializer keeps a mutation of a
/// potentially-shared value flagged.
#[must_use]
pub fn locally_owned_binding_init<'a>(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a oxc_ast::ast::Expression<'a>> {
    use oxc_ast::AstKind;

    let ref_id = ident.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    if scoping.symbol_scope_id(sym_id) == scoping.root_scope_id() {
        return None;
    }
    let decl_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    std::iter::once(nodes.kind(decl_id))
        .chain(nodes.ancestor_kinds(decl_id))
        .find_map(|kind| match kind {
            AstKind::VariableDeclarator(decl) => Some(decl.init.as_ref()),
            _ => None,
        })
        .flatten()
}

/// True when `ident` resolves to a `VariableDeclarator` binding declared in an
/// inner (non-module) scope whose initializer is an array literal (`[...]`,
/// `[]`) or a `new Array(...)` construction — a locally-owned fresh array.
///
/// Mutating such an array (`push`/`unshift`/`sort`/…) is not observable outside
/// the declaring function: the "build up a local accumulator, then return or
/// consume it" pattern (`const items = []; items.push(x); return items`). The
/// array analogue of [`is_local_object_builder_binding`].
///
/// A function parameter, imported binding, `this`, a module/root-scope binding,
/// or a non-array initializer is rejected, so a mutation of a potentially-shared
/// array stays flagged.
#[must_use]
pub fn is_locally_owned_array_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    locally_owned_binding_init(ident, semantic).is_some_and(is_array_initializer)
}

/// How a bare-identifier `return <ident>;` resolves once its binding is
/// inspected, for the sync/async-return-mixing rules. Classifying by the
/// binding's initializer — rather than assuming any bare identifier is a
/// synchronous value — is what keeps `const p = load(); return p;` (where `load`
/// returns a `Promise`) from being misread as a sync return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingReturnKind {
    /// The initializer is a Promise: an `await`, `new Promise(...)`, a
    /// `.then`/`.catch`/`.finally` chain, a `Promise.resolve/all/allSettled/race/any(...)`
    /// combinator, or a call to a function/arrow/parameter whose declared return
    /// type is `Promise<…>` (or `PromiseLike<…>`).
    Async,
    /// The initializer is a literal, template literal, array, or object — a plain
    /// synchronous value.
    Sync,
    /// The identifier does not resolve to an inner-scope `const`/`let` initializer
    /// (a parameter, import, root-scope binding, or a declarator with no
    /// initializer), or the resolved initializer's nature cannot be determined
    /// syntactically. Callers must treat this as evidence of neither a sync nor an
    /// async return.
    Unknown,
}

/// Classify a bare-identifier return by resolving the identifier to its
/// inner-scope `const`/`let` initializer and inspecting that initializer. Shared
/// by `ts-no-mixed-sync-async-returns` and `no-conditional-async-return` so a
/// Promise-bound identifier is not mistaken for a synchronous return.
#[must_use]
pub fn classify_identifier_binding_return(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> BindingReturnKind {
    match locally_owned_binding_init(ident, semantic) {
        Some(init) => classify_binding_initializer(init, semantic),
        None => BindingReturnKind::Unknown,
    }
}

/// Classify a resolved binding initializer by its syntactic shape (and, for a
/// call, the declared return type of its callee).
fn classify_binding_initializer(
    init: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> BindingReturnKind {
    use oxc_ast::ast::Expression;
    match init {
        Expression::AwaitExpression(_) => BindingReturnKind::Async,
        Expression::NewExpression(new_expr) => {
            // `new Promise(...)` is async; any other construction is a plain
            // synchronous value, matching the direct-return classifiers.
            if matches!(&new_expr.callee, Expression::Identifier(id) if id.name.as_str() == "Promise")
            {
                BindingReturnKind::Async
            } else {
                BindingReturnKind::Sync
            }
        }
        Expression::CallExpression(call) => classify_call_initializer(call, semantic),
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::TemplateLiteral(_)
        | Expression::ArrayExpression(_)
        | Expression::ObjectExpression(_) => BindingReturnKind::Sync,
        _ => BindingReturnKind::Unknown,
    }
}

/// Classify a call-expression initializer. A `.then`/`.catch`/`.finally` chain or
/// a `Promise.<combinator>(...)` (bar the `reject` error channel) is a Promise; a
/// plain call `load(...)` is a Promise only when its callee's declared return type
/// is `Promise<…>`. Anything else is unknown.
fn classify_call_initializer(
    call: &oxc_ast::ast::CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> BindingReturnKind {
    use oxc_ast::ast::Expression;
    if let Expression::StaticMemberExpression(member) = &call.callee {
        let method = member.property.name.as_str();
        if matches!(method, "then" | "catch" | "finally") {
            return BindingReturnKind::Async;
        }
        if matches!(&member.object, Expression::Identifier(obj) if obj.name.as_str() == "Promise")
            && matches!(method, "resolve" | "all" | "allSettled" | "race" | "any")
        {
            return BindingReturnKind::Async;
        }
    }
    if callee_declared_returns_promise(&call.callee, semantic) {
        return BindingReturnKind::Async;
    }
    BindingReturnKind::Unknown
}

/// True when `callee`, resolved through the symbol table, is a
/// function/arrow/parameter whose *declared* return type is `Promise<…>` (or
/// `PromiseLike<…>`), or an `async` function/arrow (whose return is always a
/// Promise). Handles a value of function type (`load: () => Promise<T>` parameter
/// or `const load: () => Promise<T>`) and a function/arrow/function-expression
/// declaration carrying the annotation. A callee with no resolvable Promise
/// return type yields `false`.
fn callee_declared_returns_promise(
    callee: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Expression, TSType};
    let Expression::Identifier(id) = callee else {
        return false;
    };
    if let Some(TSType::TSFunctionType(func_ty)) = binding_declared_ts_type(id, semantic)
        && ts_type_is_promise_ref(&func_ty.return_type.type_annotation)
    {
        return true;
    }
    let Some(ref_id) = id.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    std::iter::once(nodes.kind(decl_id))
        .chain(nodes.ancestor_kinds(decl_id))
        .find_map(|kind| match kind {
            AstKind::Function(func) => {
                Some(func.r#async || return_type_is_promise_ref(func.return_type.as_deref()))
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                Some(arrow.r#async || return_type_is_promise_ref(arrow.return_type.as_deref()))
            }
            AstKind::VariableDeclarator(decl) => decl.init.as_ref().and_then(|init| match init {
                Expression::ArrowFunctionExpression(a) => {
                    Some(a.r#async || return_type_is_promise_ref(a.return_type.as_deref()))
                }
                Expression::FunctionExpression(f) => {
                    Some(f.r#async || return_type_is_promise_ref(f.return_type.as_deref()))
                }
                _ => None,
            }),
            _ => None,
        })
        .unwrap_or(false)
}

/// True when a TS type is a `Promise<…>` / `PromiseLike<…>` type reference.
fn ts_type_is_promise_ref<'a>(ty: &'a oxc_ast::ast::TSType<'a>) -> bool {
    matches!(type_reference_name(ty), Some("Promise" | "PromiseLike"))
}

/// True when a return-type annotation, if present, is `Promise<…>` / `PromiseLike<…>`.
fn return_type_is_promise_ref(ann: Option<&oxc_ast::ast::TSTypeAnnotation>) -> bool {
    ann.is_some_and(|a| ts_type_is_promise_ref(&a.type_annotation))
}

/// True when `call` is `JSON.<method>(...)` — a `StaticMemberExpression` callee
/// whose object is the identifier `JSON` and whose property is `method`.
pub fn is_json_method_call(call: &oxc_ast::ast::CallExpression, method: &str) -> bool {
    use oxc_ast::ast::Expression;
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name.as_str() == "JSON" && member.property.name.as_str() == method
}

/// True when, at the point of a mutation starting at byte offset `mutation_start`,
/// the receiver `ident` provably holds a freshly-created local object because the
/// **nearest preceding write** to its binding reassigned it to a fresh-copy
/// expression (`is_fresh_copy_expression`):
///
/// ```ts
/// function f(options = {}) {
///   options = Object.assign({}, options); // fresh copy
///   options.showCache ??= true;           // mutates the fresh copy, not the caller's
/// }
/// ```
///
/// Considering only the *nearest* preceding write is sound: no write to the
/// binding happens between that fresh-copy reassignment and the mutation, so the
/// receiver still references the fresh object. A later reassignment to external
/// state (`options = getConfig()`) becomes the nearest preceding write for any
/// subsequent mutation and is not a fresh copy, so that mutation stays flagged.
/// A binding never reassigned to a fresh copy (a plain parameter or a `const`
/// from an external call) has no qualifying write and stays flagged.
#[must_use]
pub fn is_reassigned_fresh_copy_at(
    ident: &oxc_ast::ast::IdentifierReference,
    mutation_start: u32,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::AssignmentTarget;
    use oxc_semantic::ReferenceFlags;
    use oxc_span::GetSpan;

    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let nodes = semantic.nodes();

    // Nearest write-reference to this binding strictly before the mutation.
    let mut nearest: Option<(u32, &oxc_ast::ast::AssignmentExpression)> = None;
    for reference in scoping.get_resolved_references(sym_id) {
        if !reference.flags().contains(ReferenceFlags::Write) {
            continue;
        }
        let write_node = nodes.get_node(reference.node_id());
        let write_start = write_node.kind().span().start;
        if write_start >= mutation_start {
            continue;
        }
        // The write reference is the LHS identifier of an assignment; its parent
        // is the `AssignmentExpression`. A `let x = …` declarator is a separate
        // declaration node, not a write reference, so it is not considered here.
        let AstKind::AssignmentExpression(assign) = nodes.parent_node(write_node.id()).kind()
        else {
            continue;
        };
        if !matches!(assign.left, AssignmentTarget::AssignmentTargetIdentifier(_)) {
            continue;
        }
        if nearest.is_none_or(|(start, _)| write_start > start) {
            nearest = Some((write_start, assign));
        }
    }

    nearest.is_some_and(|(_, assign)| is_fresh_copy_expression(&assign.right))
}

/// Node module-system specifiers a `Module` binding can be imported/required from.
const NODE_MODULE_SPECIFIERS: &[&str] = &["module", "node:module"];

/// True when the mutation receiver `obj_expr` resolves to a Node.js Module-system
/// object, whose in-place mutation is the module-loader contract rather than
/// accidental mutation of shared state. A module loader (jiti, ts-node, tsx) must
/// populate `Module` instances and `Module._cache` to interoperate with native
/// `require()`. Recognised receivers:
///
/// - the `Module` builtin itself (`Module._cache[id]`, `Module.xxx`) — the base
///   identifier resolves to an import/require of `Module` from `module`/
///   `node:module`;
/// - a binding constructed as `new Module(...)` where the constructor resolves to
///   that same `Module` builtin (`const mod = new Module(f); mod.loaded = true`);
/// - a member chain reaching a Module record's `children` array via a
///   `parent`/`parentModule` segment (`ctx.parentModule.children.push(mod)`) —
///   the Node CJS dependency-graph array Node owns and the loader must mutate.
///
/// Resolution is structural (import/require binding + `new Module()` initializer),
/// never a bare member-name match on a foreign object: a `cache[id] = …` on an
/// ordinary `cache` object, or a `mod.x = …` on a binding that is not a
/// `new Module()`, stays flagged.
#[must_use]
pub fn is_node_module_system_target(
    obj_expr: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    // `<base>.children` reached through a `parentModule`/`parent` segment — the
    // Module dependency-graph array (`ctx.parentModule.children.push(mod)`).
    if member_chain_reaches_module_children(obj_expr, semantic) {
        return true;
    }

    // Base identifier of the receiver chain.
    let Some(root) = receiver_root_identifier_ref(obj_expr) else {
        return false;
    };
    // `Module._cache[...]`, `Module.xxx` — the builtin itself.
    if root.name.as_str() == "Module" && resolves_to_node_module_ctor(root, semantic) {
        return true;
    }
    // A `const mod = new Module(...)` instance — any property mutation on it.
    is_node_module_instance_binding(root, semantic)
}

/// Leftmost `IdentifierReference` of a member chain, preserving the reference so
/// its binding can be resolved. `Module._cache[id]` → the `Module` reference.
fn receiver_root_identifier_ref<'a>(
    expr: &'a oxc_ast::ast::Expression<'a>,
) -> Option<&'a oxc_ast::ast::IdentifierReference<'a>> {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::Identifier(id) => Some(id),
        Expression::StaticMemberExpression(m) => receiver_root_identifier_ref(&m.object),
        Expression::ComputedMemberExpression(m) => receiver_root_identifier_ref(&m.object),
        _ => None,
    }
}

/// True when the receiver chain ends in a `.children` access reached through a
/// Module-record segment — the Node CJS dependency-graph array:
/// - `<base>.parentModule.children` — `parentModule` is the Node module-loader
///   convention for a parent `Module` record, so the name alone qualifies
///   (`ctx.parentModule.children.push(mod)`);
/// - `<base>.parent.children` — `parent` is a generic name (DOM/AST/tree
///   walkers use `node.parent.children.push(...)`), so it qualifies only when
///   the chain's base identifier resolves to a `new Module(...)` instance
///   (`mod.parent.children.push(...)`).
///
/// A bare `obj.children` (no `parent`/`parentModule` segment) and a foreign
/// `node.parent.children` (base not a Module instance) do not match.
fn member_chain_reaches_module_children(
    expr: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::Expression;
    let Expression::StaticMemberExpression(children_member) = expr else {
        return false;
    };
    if children_member.property.name.as_str() != "children" {
        return false;
    }
    let Expression::StaticMemberExpression(parent_member) = &children_member.object else {
        return false;
    };
    match parent_member.property.name.as_str() {
        "parentModule" => true,
        "parent" => receiver_root_identifier_ref(&parent_member.object)
            .is_some_and(|root| is_node_module_instance_binding(root, semantic)),
        _ => false,
    }
}

/// True when `ident` resolves to a `const`/`let` binding whose initializer is
/// `new Module(...)` and that `Module` constructor resolves to the Node
/// module-system builtin.
fn is_node_module_instance_binding(
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
            let Some(Expression::NewExpression(new_expr)) = &decl.init else {
                return false;
            };
            let Expression::Identifier(ctor) = &new_expr.callee else {
                return false;
            };
            return ctor.name.as_str() == "Module"
                && resolves_to_node_module_ctor(ctor, semantic);
        }
    }
    false
}

/// True when `ident` (named `Module`) resolves to a binding imported or required
/// from `module` / `node:module`: an ESM `import { Module } from "node:module"`,
/// or a CommonJS `const { Module } = require("module")` /
/// `const Module = require("module").Module`.
fn resolves_to_node_module_ctor(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Argument, Expression};

    if resolves_to_import_from(ident, semantic, NODE_MODULE_SPECIFIERS) {
        return true;
    }

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
            // `require("module")` or `require("module").Module`.
            let require_call = match init {
                Expression::CallExpression(call) => Some(call.as_ref()),
                Expression::StaticMemberExpression(member) => match &member.object {
                    Expression::CallExpression(call) => Some(call.as_ref()),
                    _ => None,
                },
                _ => None,
            };
            let Some(call) = require_call else {
                return false;
            };
            let is_require =
                matches!(&call.callee, Expression::Identifier(id) if id.name == "require");
            return is_require
                && matches!(call.arguments.first(), Some(Argument::StringLiteral(lit))
                    if NODE_MODULE_SPECIFIERS.contains(&lit.value.as_str()));
        }
    }
    false
}

/// True when `ident` resolves to a local `const`/`let` binding whose initializer
/// is a freshly allocated array — a `CallExpression` (`rollups(...)`,
/// `nodes.leaves().map(...)`) or an `ArrayExpression` (`[...]`) — and that array
/// never escapes as a bare value before being read here.
///
/// This is the `let arr = fn(); arr.sort()` builder pattern: `arr` holds the same
/// fresh call result as the exempted direct chain `fn().sort()`, just bound to a
/// name for readability. No caller holds a reference to it, so an in-place `.sort()`
/// mutation is unobservable, exactly as in the inline case.
///
/// The escape guard requires every resolved reference of the binding to read
/// through it without leaking a pre-sort alias: the **object of a member access**
/// (`arr.map(...)`, `arr.length`, `arr[i]`) or the iterated expression of a
/// `for…of` loop (`for (const x of arr)`), which only consumes the iterator. A
/// `return <binding>` is additionally allowed, but only when the initializer is a
/// provably-fresh built-in Array copy (`x.slice(…)`, `.filter`/`.map`/`Array.from`/
/// `[...x]`, …): the value handed out is the already-sorted private copy, so no
/// pre-existing alias can observe the reorder. Any other bare-value use — a call
/// argument (`use(arr)`), an assignment source (`x = arr`), a spread (`[...arr]`),
/// an object-property value (`{ k: arr }`), or a `return` of an opaque-call binding
/// (`const xs = getItems(); return xs`, whose result may be shared) — could hand
/// the array to code that observes the reorder, so the binding is rejected and the
/// `.sort()` stays flagged.
///
/// A function parameter, imported binding, or `this` resolves to a
/// non-`VariableDeclarator` declaration; a `var` binding or a non-array
/// initializer is rejected, so a shared reference is still flagged.
#[must_use]
pub fn is_local_fresh_array_binding(
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

    let mut is_fresh_local = false;
    let mut init_is_fresh_copy = false;
    for kind in
        std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            // `(await fn()) as T[]`, `fn()!`, `fn() satisfies T` all evaluate to
            // the same array the inner call/literal produces, so peel those
            // transparent wrappers before deciding freshness.
            let init = decl.init.as_ref().map(peel_value_wrappers);
            is_fresh_local = matches!(
                decl.kind,
                VariableDeclarationKind::Const | VariableDeclarationKind::Let
            ) && matches!(
                init,
                Some(Expression::CallExpression(_) | Expression::ArrayExpression(_))
            );
            init_is_fresh_copy =
                is_fresh_local && init.is_some_and(initializer_is_fresh_array_copy);
            break;
        }
    }
    if !is_fresh_local {
        return false;
    }

    // The array must not leak a pre-sort alias. Every reference reads through the
    // binding (member object, `for…of` iterable), and — only for a provably-fresh
    // built-in copy — a `return <binding>` may hand out the already-sorted copy.
    scoping.get_resolved_references(sym_id).all(|reference| {
        reference_is_member_object(reference.node_id(), semantic, init_is_fresh_copy)
    })
}

/// True when the reference at `ref_node_id` cannot expose a pre-sort alias of the
/// binding: it is the *object* of a member access (`arr.foo`, `arr[i]`), the
/// iterated expression of a `for…of` loop (`for (const x of arr)`), which consumes
/// the iterator without retaining the array, or — when `allow_return_escape` is set
/// (the initializer is a provably-fresh built-in copy) — the argument of a `return`,
/// which hands out the already-sorted private copy. Any other parent (call
/// argument, assignment source, spread, property value, the `for…of` binding
/// target, or a `return` of a possibly-shared binding) lets a pre-sort alias escape
/// and returns `false`.
fn reference_is_member_object(
    ref_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
    allow_return_escape: bool,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;

    let nodes = semantic.nodes();
    let ref_span = nodes.get_node(ref_node_id).kind().span();
    match nodes.kind(nodes.parent_id(ref_node_id)) {
        AstKind::StaticMemberExpression(member) => member.object.span() == ref_span,
        AstKind::ComputedMemberExpression(member) => member.object.span() == ref_span,
        AstKind::ForOfStatement(for_of) => for_of.right.span() == ref_span,
        AstKind::ReturnStatement(ret) => {
            allow_return_escape && ret.argument.as_ref().is_some_and(|arg| arg.span() == ref_span)
        }
        _ => false,
    }
}

/// Built-in `Array` methods that always return a brand-new array, leaving the
/// receiver untouched, so a binding initialised by one of them holds a private
/// copy no other code can alias. Excludes the in-place mutators (`sort`,
/// `reverse`, `splice`, `fill`), whose result is the receiver itself.
const FRESH_ARRAY_COPY_METHODS: &[&str] = &[
    "slice", "filter", "map", "concat", "flat", "flatMap", "toSorted", "toReversed", "with",
];

/// True when `init` provably evaluates to a freshly allocated array no other code
/// can alias: a call to a built-in copy method (`x.slice(…)`, `.filter`, `.map`, …),
/// `Array.from(…)`, or an array literal with a spread (`[...x]`). A `return` of a
/// binding with such an initializer only exposes the sorted copy, unlike an opaque
/// call (`getItems()`) whose result may be a shared array.
fn initializer_is_fresh_array_copy(init: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::{ArrayExpressionElement, Expression};

    match init {
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(callee) = &call.callee else {
                return false;
            };
            let method = callee.property.name.as_str();
            FRESH_ARRAY_COPY_METHODS.contains(&method)
                || (method == "from"
                    && matches!(&callee.object, Expression::Identifier(obj) if obj.name.as_str() == "Array"))
        }
        Expression::ArrayExpression(array) => array
            .elements
            .iter()
            .any(|el| matches!(el, ArrayExpressionElement::SpreadElement(_))),
        _ => false,
    }
}

/// True when `member` is `<el>.innerHTML` and `<el>` is a detached element minted
/// by `document.createElement(...)` that is used solely as an HTML→text parser:
/// the only references to `<el>` are this single `.innerHTML` write and reads of
/// `.textContent` / `.innerText`. Such an element never reaches the live DOM, so
/// the assignment parses HTML into plain text rather than feeding an XSS sink.
///
/// Conservative by construction (bias to flag): the binding must be a `const`
/// initialised from `document.createElement(...)`, and EVERY one of its resolved
/// references must be either this exact `.innerHTML` write (matched by span) or a
/// `.textContent` / `.innerText` member access. ANY other reference — an
/// insertion-method receiver (`el.appendChild(...)`), a call argument
/// (`document.body.appendChild(el)`), a `return el`, a reassignment, a second
/// `.innerHTML` write, a read of `.innerHTML`, a computed access, or any other
/// property — makes this return `false` so the write keeps flagging. A non-`const`
/// binding or a non-`createElement` origin (parameter, `getElementById`,
/// `querySelector`, member chain) also returns `false`.
#[must_use]
pub fn assignment_target_is_detached_text_parser(
    member: &oxc_ast::ast::StaticMemberExpression,
    semantic: &Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Expression, VariableDeclarationKind};

    if member.property.name.as_str() != "innerHTML" {
        return false;
    }
    let Expression::Identifier(ident) = &member.object else {
        return false;
    };
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let nodes = semantic.nodes();
    // Origin must be `const el = document.createElement(...)`. A `let`/`var`
    // binding, a parameter, or any non-`createElement` initializer fails closed.
    let AstKind::VariableDeclarator(decl) = nodes.kind(scoping.symbol_declaration(sym_id)) else {
        return false;
    };
    if decl.kind != VariableDeclarationKind::Const
        || !decl.init.as_ref().is_some_and(initializer_is_create_element)
    {
        return false;
    }
    // The flagged write is identified by its span, so a *different* `.innerHTML`
    // access on `el` (a second write or a read) is rejected as a non-text use.
    // Exempt only when EVERY reference is the write or a `.textContent`/
    // `.innerText` read AND at least one such read exists — positive evidence the
    // element is an HTML→text parser, not a bare `div.innerHTML = x` sink.
    let write_span = member.span;
    let mut saw_text_read = false;
    for reference in scoping.get_resolved_references(sym_id) {
        match classify_detached_reference(reference.node_id(), write_span, semantic) {
            DetachedRefUse::TextRead => saw_text_read = true,
            DetachedRefUse::Write => {}
            DetachedRefUse::Escape => return false,
        }
    }
    saw_text_read
}

/// How a reference to a candidate detached-parser element is used.
enum DetachedRefUse {
    /// A `.textContent` / `.innerText` member access — plain-text extraction.
    TextRead,
    /// The single flagged `.innerHTML` write (matched by span).
    Write,
    /// Anything else — the element escapes (call argument, return, reassignment,
    /// computed access) or acts as a sink (a different property, a second
    /// `.innerHTML` access).
    Escape,
}

/// Classify the reference at `ref_node_id`: a `.textContent`/`.innerText` read,
/// the `.innerHTML` write identified by `write_span`, or an escape/sink use.
fn classify_detached_reference(
    ref_node_id: oxc_semantic::NodeId,
    write_span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic,
) -> DetachedRefUse {
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;

    let nodes = semantic.nodes();
    let ref_span = nodes.get_node(ref_node_id).kind().span();
    let AstKind::StaticMemberExpression(member) = nodes.kind(nodes.parent_id(ref_node_id)) else {
        return DetachedRefUse::Escape;
    };
    // The reference must be the receiver of the member access, not a nested use.
    if member.object.span() != ref_span {
        return DetachedRefUse::Escape;
    }
    match member.property.name.as_str() {
        "textContent" | "innerText" => DetachedRefUse::TextRead,
        "innerHTML" if member.span == write_span => DetachedRefUse::Write,
        _ => DetachedRefUse::Escape,
    }
}

/// Whether an initializer is `document.createElement(<tag>)` — the DOM API that
/// mints a fresh, detached element (object `document`, property `createElement`).
fn initializer_is_create_element(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(callee) = &call.callee else {
        return false;
    };
    callee.property.name.as_str() == "createElement"
        && matches!(&callee.object, Expression::Identifier(obj) if obj.name.as_str() == "document")
}

/// Calls whose result is an array: `[...].map(...)`, `Object.keys(o)`,
/// `Array.from(x)`, `str.split(...)`, etc. Matched on the member/static method
/// name of the callee.
const ARRAY_PRODUCING_METHODS: &[&str] = &[
    "map", "filter", "slice", "splice", "concat", "flat", "flatMap", "split", "sort", "reverse",
    "fill", "from", "of", "keys", "values", "entries", "toSorted", "toReversed", "toSpliced",
    "with", "getOwnPropertyNames",
];

/// Whether a type annotation denotes an array: `T[]`, `readonly T[]`,
/// `Array<T>`, `ReadonlyArray<T>`.
fn type_is_array(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
    match ty {
        TSType::TSArrayType(_) => true,
        TSType::TSTypeOperatorType(op) if op.operator == TSTypeOperatorOperator::Readonly => {
            type_is_array(&op.type_annotation)
        }
        TSType::TSTypeReference(tref) => matches!(
            &tref.type_name,
            TSTypeName::IdentifierReference(id)
                if matches!(id.name.as_str(), "Array" | "ReadonlyArray")
        ),
        _ => false,
    }
}

/// Whether a call's callee is an array-producing method (`x.map`, `Object.keys`,
/// `Array.from`).
fn callee_produces_array(callee: &oxc_ast::ast::Expression) -> bool {
    let oxc_ast::ast::Expression::StaticMemberExpression(member) = callee else {
        return false;
    };
    ARRAY_PRODUCING_METHODS.contains(&member.property.name.as_str())
}

/// Whether an initializer expression evaluates to an array: an array literal,
/// `new Array(...)`, or an array-producing method/static call.
fn initializer_is_array(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::ArrayExpression(_) => true,
        Expression::NewExpression(new_expr) => matches!(
            &new_expr.callee,
            Expression::Identifier(id) if id.name.as_str() == "Array"
        ),
        Expression::CallExpression(call) => callee_produces_array(&call.callee),
        Expression::ParenthesizedExpression(paren) => initializer_is_array(&paren.expression),
        Expression::TSAsExpression(as_expr) => {
            type_is_array(&as_expr.type_annotation) || initializer_is_array(&as_expr.expression)
        }
        Expression::TSSatisfiesExpression(sat) => initializer_is_array(&sat.expression),
        Expression::TSNonNullExpression(nn) => initializer_is_array(&nn.expression),
        _ => false,
    }
}

/// Resolve an identifier reference to its declaration and decide whether that
/// declaration proves the binding holds an array — a `let`/`const`/`var`
/// declarator carrying an array type annotation or initialised from an
/// array-producing expression, or a parameter typed as an array.
fn binding_is_array(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    let scoping = semantic.scoping();
    let Some(symbol_id) = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
    else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(symbol_id);
    match nodes.kind(decl_id) {
        AstKind::VariableDeclarator(decl) => {
            if let Some(type_ann) = &decl.type_annotation
                && type_is_array(&type_ann.type_annotation)
            {
                return true;
            }
            decl.init.as_ref().is_some_and(initializer_is_array)
        }
        AstKind::FormalParameter(param) => param
            .type_annotation
            .as_ref()
            .is_some_and(|ann| type_is_array(&ann.type_annotation)),
        _ => false,
    }
}

/// Whether `expr` is demonstrably an array. An array literal is one directly; an
/// array-producing expression (`new Array(...)`, `[...].map()`, `Array.from(x)`,
/// `Object.keys(o)`, `str.split(...)`) is one; an identifier is one only if its
/// binding carries an array type annotation (`T[]`/`readonly T[]`/`Array<T>`/
/// `ReadonlyArray<T>`) or is initialised from an array-producing expression.
///
/// A receiver's *name* is never evidence — names do not determine type. A
/// receiver whose type cannot be proven an array (an untyped parameter, a
/// member-access chain, a call returning an unknown shape) returns `false`, so
/// callers that gate on this predicate do not flag method-name collisions on
/// non-array objects (e.g. a canvas `shape.fill(color)` color-setter).
#[must_use]
pub fn expression_is_array(
    expr: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::ArrayExpression(_) => true,
        Expression::NewExpression(_) | Expression::CallExpression(_) => initializer_is_array(expr),
        Expression::Identifier(ident) => binding_is_array(ident, semantic),
        Expression::ParenthesizedExpression(paren) => {
            expression_is_array(&paren.expression, semantic)
        }
        Expression::TSAsExpression(as_expr) => {
            type_is_array(&as_expr.type_annotation)
                || expression_is_array(&as_expr.expression, semantic)
        }
        Expression::TSSatisfiesExpression(sat) => expression_is_array(&sat.expression, semantic),
        Expression::TSNonNullExpression(nn) => expression_is_array(&nn.expression, semantic),
        _ => false,
    }
}

/// Whether a type annotation denotes a built-in keyed map: `Map<K, V>`,
/// `WeakMap<K, V>`, or `ReadonlyMap<K, V>`. These are the standard library
/// containers whose `.set(key, value)` merely stores a value at `key` with no
/// observable side effect, so a same-key re-`set` overwrites a dead store.
fn type_is_map(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    let TSType::TSTypeReference(tref) = ty else {
        return false;
    };
    matches!(
        &tref.type_name,
        TSTypeName::IdentifierReference(id)
            if matches!(id.name.as_str(), "Map" | "WeakMap" | "ReadonlyMap")
    )
}

/// Whether an initializer expression evaluates to a built-in map: `new Map(...)`
/// or `new WeakMap(...)`.
fn initializer_is_map(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::NewExpression(new_expr) => matches!(
            &new_expr.callee,
            Expression::Identifier(id) if matches!(id.name.as_str(), "Map" | "WeakMap")
        ),
        Expression::ParenthesizedExpression(paren) => initializer_is_map(&paren.expression),
        Expression::TSAsExpression(as_expr) => {
            type_is_map(&as_expr.type_annotation) || initializer_is_map(&as_expr.expression)
        }
        Expression::TSSatisfiesExpression(sat) => initializer_is_map(&sat.expression),
        Expression::TSNonNullExpression(nn) => initializer_is_map(&nn.expression),
        _ => false,
    }
}

/// Resolve an identifier reference to its declaration and decide whether that
/// declaration proves the binding holds a built-in map — a `let`/`const`/`var`
/// declarator carrying a `Map`/`WeakMap`/`ReadonlyMap` type annotation or
/// initialised from `new Map()`/`new WeakMap()`, or a parameter typed as one.
fn binding_is_map(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    let scoping = semantic.scoping();
    let Some(symbol_id) = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
    else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(symbol_id);
    match nodes.kind(decl_id) {
        AstKind::VariableDeclarator(decl) => {
            if let Some(type_ann) = &decl.type_annotation
                && type_is_map(&type_ann.type_annotation)
            {
                return true;
            }
            decl.init.as_ref().is_some_and(initializer_is_map)
        }
        AstKind::FormalParameter(param) => param
            .type_annotation
            .as_ref()
            .is_some_and(|ann| type_is_map(&ann.type_annotation)),
        _ => false,
    }
}

/// Whether `expr` is demonstrably a built-in `Map`/`WeakMap`. A `new Map(...)`
/// expression is one directly; an identifier is one only if its binding carries
/// a `Map`/`WeakMap`/`ReadonlyMap` type annotation or is initialised from
/// `new Map()`/`new WeakMap()`.
///
/// A receiver's *name* is never evidence — names do not determine type. A
/// receiver whose type cannot be proven a map (a state-store binding, a member
/// chain, an untyped parameter) returns `false`. Callers gate `.set(key, value)`
/// overwrite detection on this so a dispatch-style `.set` with side effects
/// (e.g. a jotai `store.set(atom, value)`) is not treated as a dead store.
#[must_use]
pub fn expression_is_map(
    expr: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::NewExpression(_) => initializer_is_map(expr),
        Expression::Identifier(ident) => binding_is_map(ident, semantic),
        Expression::ParenthesizedExpression(paren) => {
            expression_is_map(&paren.expression, semantic)
        }
        Expression::TSAsExpression(as_expr) => {
            type_is_map(&as_expr.type_annotation)
                || expression_is_map(&as_expr.expression, semantic)
        }
        Expression::TSSatisfiesExpression(sat) => expression_is_map(&sat.expression, semantic),
        Expression::TSNonNullExpression(nn) => expression_is_map(&nn.expression, semantic),
        _ => false,
    }
}

/// Whether a type annotation denotes a built-in keyed set: `Set<T>`,
/// `WeakSet<T>`, or `ReadonlySet<T>`.
fn type_is_set(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    let TSType::TSTypeReference(tref) = ty else {
        return false;
    };
    matches!(
        &tref.type_name,
        TSTypeName::IdentifierReference(id)
            if matches!(id.name.as_str(), "Set" | "WeakSet" | "ReadonlySet")
    )
}

/// Whether an initializer expression evaluates to a built-in set: `new Set(...)`
/// or `new WeakSet(...)`.
fn initializer_is_set(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::NewExpression(new_expr) => matches!(
            &new_expr.callee,
            Expression::Identifier(id) if matches!(id.name.as_str(), "Set" | "WeakSet")
        ),
        Expression::ParenthesizedExpression(paren) => initializer_is_set(&paren.expression),
        Expression::TSAsExpression(as_expr) => {
            type_is_set(&as_expr.type_annotation) || initializer_is_set(&as_expr.expression)
        }
        Expression::TSSatisfiesExpression(sat) => initializer_is_set(&sat.expression),
        Expression::TSNonNullExpression(nn) => initializer_is_set(&nn.expression),
        _ => false,
    }
}

/// Resolve an identifier reference to its declaration and decide whether that
/// declaration proves the binding holds a built-in set — a `let`/`const`/`var`
/// declarator carrying a `Set`/`WeakSet`/`ReadonlySet` type annotation or
/// initialised from `new Set()`/`new WeakSet()`, or a parameter typed as one.
fn binding_is_set(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    let scoping = semantic.scoping();
    let Some(symbol_id) = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
    else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(symbol_id);
    match nodes.kind(decl_id) {
        AstKind::VariableDeclarator(decl) => {
            if let Some(type_ann) = &decl.type_annotation
                && type_is_set(&type_ann.type_annotation)
            {
                return true;
            }
            decl.init.as_ref().is_some_and(initializer_is_set)
        }
        AstKind::FormalParameter(param) => param
            .type_annotation
            .as_ref()
            .is_some_and(|ann| type_is_set(&ann.type_annotation)),
        _ => false,
    }
}

/// Whether `expr` is demonstrably a built-in `Set`/`WeakSet`. A `new Set(...)`
/// expression is one directly; an identifier is one only if its binding carries
/// a `Set`/`WeakSet`/`ReadonlySet` type annotation or is initialised from
/// `new Set()`/`new WeakSet()`.
///
/// A receiver's *name* is never evidence — names do not determine type. A
/// receiver whose type cannot be proven a set (a `String(...)` call, a string
/// literal, an array, a member chain, an untyped parameter) returns `false`.
#[must_use]
pub fn expression_is_set(
    expr: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::NewExpression(_) => initializer_is_set(expr),
        Expression::Identifier(ident) => binding_is_set(ident, semantic),
        Expression::ParenthesizedExpression(paren) => {
            expression_is_set(&paren.expression, semantic)
        }
        Expression::TSAsExpression(as_expr) => {
            type_is_set(&as_expr.type_annotation)
                || expression_is_set(&as_expr.expression, semantic)
        }
        Expression::TSSatisfiesExpression(sat) => expression_is_set(&sat.expression, semantic),
        Expression::TSNonNullExpression(nn) => expression_is_set(&nn.expression, semantic),
        _ => false,
    }
}

/// True when `arg` is `delete recv.prop` whose `prop` is declared **optional**
/// (`prop?: T`) on the receiver's structurally-resolved named type. Deleting an
/// optional member returns the object to the absent state its own type already
/// permits, so it is type-safe and intentional — not the foot-gun the rule
/// targets (deleting a required field, leaving a hole the type forbids).
///
/// Both halves are resolved **structurally**, never from the property name:
/// - the receiver's named type comes from an `as`/`satisfies` assertion
///   (`(v as Memo<any>).tOwned`), a directly-annotated binding (`v: Memo`), or a
///   `for-of` loop variable iterating an array/`Set` of a named element type
///   (`for (const e of effects)` where `effects: Computation[]`);
/// - the property's optionality comes from the matching `TSPropertySignature`
///   on the named `interface`/object-`type` declaration in the same module,
///   following `extends` heritage by name.
///
/// A computed delete (`delete obj["x"]`), a required member (`prop: T`), or a
/// receiver whose named type cannot be resolved structurally all return `false`
/// and stay flagged.
#[must_use]
pub fn is_optional_member_delete(
    arg: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::Expression;
    let Expression::StaticMemberExpression(member) = arg else {
        return false;
    };
    let Some(type_name) = receiver_named_type(&member.object, semantic) else {
        return false;
    };
    named_type_has_optional_property(type_name, member.property.name.as_str(), semantic, 0)
}

/// Maximum heritage/alias hops walked while resolving an optional member, so a
/// cyclic or pathological `extends` chain cannot loop forever.
const OPTIONAL_MEMBER_RESOLUTION_DEPTH: u32 = 8;

/// The name of the receiver's declared type, when it can be resolved
/// structurally. Recognizes an `as`/`satisfies` assertion to a named type, a
/// binding annotated with a named type, and a `for-of` loop variable whose
/// iterable is an array or `Set` of a named element type. Returns `None` for any
/// receiver whose type is not a single named reference (anonymous object types,
/// unions, inferred bindings).
fn receiver_named_type<'a>(
    expr: &'a oxc_ast::ast::Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::TSAsExpression(as_expr) => type_reference_name(&as_expr.type_annotation),
        Expression::TSSatisfiesExpression(sat) => type_reference_name(&sat.type_annotation),
        Expression::ParenthesizedExpression(paren) => {
            receiver_named_type(&paren.expression, semantic)
        }
        Expression::Identifier(ident) => binding_named_type(ident, semantic),
        _ => None,
    }
}

/// The identifier name of a `T` / `Array<T>`-style type reference, or `None` for
/// any non-reference type.
fn type_reference_name<'a>(ty: &'a oxc_ast::ast::TSType<'a>) -> Option<&'a str> {
    use oxc_ast::ast::{TSType, TSTypeName};
    let TSType::TSTypeReference(tref) = ty else {
        return None;
    };
    match &tref.type_name {
        TSTypeName::IdentifierReference(id) => Some(id.name.as_str()),
        TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => None,
    }
}

/// The named type of a binding: a declarator/parameter annotated with a named
/// type, or a `for-of` loop variable over an array/`Set` of a named element type.
fn binding_named_type<'a>(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    use oxc_ast::AstKind;
    let scoping = semantic.scoping();
    let symbol_id = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())?;
    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(symbol_id);
    match nodes.kind(decl_id) {
        AstKind::VariableDeclarator(decl) => decl
            .type_annotation
            .as_ref()
            .and_then(|ann| type_reference_name(&ann.type_annotation))
            .or_else(|| for_of_element_type(decl_id, semantic)),
        AstKind::FormalParameter(param) => param
            .type_annotation
            .as_ref()
            .and_then(|ann| type_reference_name(&ann.type_annotation)),
        _ => None,
    }
}

/// When `decl_id` is the declarator of a `for-of` loop variable, the named
/// element type of the iterable: `for (const e of effects)` where `effects`
/// resolves to `Computation[]` / `Array<Computation>` / `Set<Computation>`
/// yields `"Computation"`. Returns `None` when the declarator is not a `for-of`
/// binding or the iterable's element type is not a single named reference.
fn for_of_element_type<'a>(
    decl_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    use oxc_ast::AstKind;
    let nodes = semantic.nodes();
    let for_of = nodes.ancestor_kinds(decl_id).find_map(|kind| match kind {
        AstKind::ForOfStatement(stmt) => Some(stmt),
        _ => None,
    })?;
    iterable_element_type(&for_of.right, semantic)
}

/// The named element type of an iterable expression. An `as`/non-null/paren
/// wrapper is peeled; an identifier is resolved to its annotated array/`Set`
/// type. Returns `None` unless the element type is a single named reference.
fn iterable_element_type<'a>(
    expr: &'a oxc_ast::ast::Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::TSNonNullExpression(nn) => iterable_element_type(&nn.expression, semantic),
        Expression::ParenthesizedExpression(paren) => {
            iterable_element_type(&paren.expression, semantic)
        }
        Expression::TSAsExpression(as_expr) => type_element_name(&as_expr.type_annotation),
        Expression::Identifier(ident) => {
            let scoping = semantic.scoping();
            let symbol_id = ident
                .reference_id
                .get()
                .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())?;
            let nodes = semantic.nodes();
            let decl_id = scoping.symbol_declaration(symbol_id);
            let ann = match nodes.kind(decl_id) {
                oxc_ast::AstKind::VariableDeclarator(decl) => decl.type_annotation.as_ref(),
                oxc_ast::AstKind::FormalParameter(param) => param.type_annotation.as_ref(),
                _ => None,
            }?;
            type_element_name(&ann.type_annotation)
        }
        _ => None,
    }
}

/// The named element type of an array/`Set` type: `T[]`, `readonly T[]`,
/// `Array<T>`, `ReadonlyArray<T>`, `Set<T>`, `ReadonlySet<T>` → `"T"` (only when
/// `T` is itself a single named reference). A `T[] | null` union is peeled to its
/// array member. Returns `None` for any other type.
fn type_element_name<'a>(ty: &'a oxc_ast::ast::TSType<'a>) -> Option<&'a str> {
    use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
    match ty {
        TSType::TSArrayType(arr) => type_reference_name(&arr.element_type),
        TSType::TSTypeOperatorType(op) if op.operator == TSTypeOperatorOperator::Readonly => {
            type_element_name(&op.type_annotation)
        }
        TSType::TSTypeReference(tref) => {
            let TSTypeName::IdentifierReference(id) = &tref.type_name else {
                return None;
            };
            if !matches!(
                id.name.as_str(),
                "Array" | "ReadonlyArray" | "Set" | "ReadonlySet"
            ) {
                return None;
            }
            tref.type_arguments
                .as_ref()
                .and_then(|args| args.params.first())
                .and_then(type_reference_name)
        }
        TSType::TSUnionType(union) => union.types.iter().find_map(type_element_name),
        _ => None,
    }
}

/// True when the `interface`/object-`type` named `type_name` declared in this
/// module has `prop` as an **optional** property signature (`prop?: T`),
/// following `extends` heritage by name up to a bounded depth. A required
/// property, an unknown type name, or an exhausted depth budget returns `false`.
fn named_type_has_optional_property(
    type_name: &str,
    prop: &str,
    semantic: &oxc_semantic::Semantic,
    depth: u32,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Expression, TSType, TSTypeName};
    if depth >= OPTIONAL_MEMBER_RESOLUTION_DEPTH {
        return false;
    }
    for node in semantic.nodes().iter() {
        match node.kind() {
            AstKind::TSInterfaceDeclaration(decl) if decl.id.name.as_str() == type_name => {
                if signatures_have_optional_property(&decl.body.body, prop) {
                    return true;
                }
                for heritage in &decl.extends {
                    if let Expression::Identifier(base) = &heritage.expression
                        && named_type_has_optional_property(
                            base.name.as_str(),
                            prop,
                            semantic,
                            depth + 1,
                        )
                    {
                        return true;
                    }
                }
            }
            AstKind::TSTypeAliasDeclaration(decl) if decl.id.name.as_str() == type_name => {
                match &decl.type_annotation {
                    TSType::TSTypeLiteral(lit) => {
                        if signatures_have_optional_property(&lit.members, prop) {
                            return true;
                        }
                    }
                    // A `type X = Y` alias to another named type — follow it.
                    TSType::TSTypeReference(tref) => {
                        if let TSTypeName::IdentifierReference(id) = &tref.type_name
                            && named_type_has_optional_property(
                                id.name.as_str(),
                                prop,
                                semantic,
                                depth + 1,
                            )
                        {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    false
}

/// True when `signatures` contains an optional, non-computed property signature
/// named `prop`.
fn signatures_have_optional_property(
    signatures: &[oxc_ast::ast::TSSignature],
    prop: &str,
) -> bool {
    use oxc_ast::ast::{PropertyKey, TSSignature};
    signatures.iter().any(|sig| {
        let TSSignature::TSPropertySignature(p) = sig else {
            return false;
        };
        p.optional
            && !p.computed
            && match &p.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str() == prop,
                PropertyKey::StringLiteral(s) => s.value.as_str() == prop,
                _ => false,
            }
    })
}

/// True when `expr` is a **statically-bounded** array — one whose element count
/// is fixed at compile time, so spreading it (`Math.max(...expr)`) cannot exhaust
/// the engine's argument-count limit. Recognizes three structural shapes:
///
/// - an array literal with no spread element (`[a, b, c]`) — arity is the literal's
///   element count;
/// - a length-non-increasing member-call chain (`.map` / `.filter` / `.slice`)
///   rooted at one of the recognized bounded shapes (`[a, b].map(f)`,
///   `corners.filter(g).map(h)`) — none of these methods can produce *more*
///   elements than its receiver, so a bounded root stays bounded;
/// - an identifier binding resolving to a `VariableDeclarator` that is either
///   - typed as a fixed-length tuple with no rest element
///     (`const corners: [P, P, P, P]`; a `[P, ...P[]]` rest tuple is unbounded
///     and does *not* qualify) — the type is the binding's contract, so this
///     holds even for a `let`; or
///   - a `const` whose initializer is one of the bounded shapes above and which
///     is never grown in place (`arr.push`/`unshift`/`splice`). `const` rules
///     out reassignment to a dynamic array, and the growth-method check rules
///     out a `const arr = []; …arr.push(x)` accumulator — either would make the
///     arity unknown.
///
/// Returns `false` for a dynamic/unbounded array — a `number[]` / `Array<T>`
/// binding, a function-return value, a fetched or accumulated list, or a `.map`
/// rooted at any of those — so a genuine stack-overflow risk stays detectable.
/// `.flatMap` / `.concat` are excluded because they can grow the result beyond
/// the receiver's length.
#[must_use]
pub fn expression_is_statically_bounded_array(
    expr: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{
        ArrayExpressionElement, Expression, TSTupleElement, TSType, VariableDeclarationKind,
    };

    match expr {
        // `[a, b, c]` — arity known, as long as no inner spread reintroduces an
        // unbounded element (`[...rest]`).
        Expression::ArrayExpression(arr) => !arr
            .elements
            .iter()
            .any(|el| matches!(el, ArrayExpressionElement::SpreadElement(_))),

        // `<root>.map(f)` / `.filter(g)` / `.slice(...)` — length-non-increasing;
        // recurse into the receiver.
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            if !matches!(member.property.name.as_str(), "map" | "filter" | "slice") {
                return false;
            }
            expression_is_statically_bounded_array(&member.object, semantic)
        }

        // An identifier binding: resolve to its declarator and inspect the
        // initializer (literal / bounded `.map`-chain) or a fixed-length tuple
        // type annotation.
        Expression::Identifier(ident) => {
            let Some(ref_id) = ident.reference_id.get() else {
                return false;
            };
            let scoping = semantic.scoping();
            let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
                return false;
            };
            let decl_node_id = scoping.symbol_declaration(sym_id);
            let nodes = semantic.nodes();
            for kind in std::iter::once(nodes.kind(decl_node_id))
                .chain(nodes.ancestor_kinds(decl_node_id))
            {
                if let AstKind::VariableDeclarator(decl) = kind {
                    // A fixed-length tuple annotation (no rest element) is the
                    // binding's contract: it cannot hold more than its arity even
                    // for a `let`, so it qualifies regardless of reassignment.
                    if decl.type_annotation.as_ref().is_some_and(|ann| {
                        matches!(
                            &ann.type_annotation,
                            TSType::TSTupleType(tuple)
                                if !tuple.element_types.iter().any(|el| {
                                    matches!(el, TSTupleElement::TSRestType(_))
                                })
                        )
                    }) {
                        return true;
                    }
                    // A bounded-array initializer only proves boundedness while
                    // the binding keeps that value: it must be `const` (no
                    // reassignment to a dynamic array) and never grown in place
                    // (`arr.push(...)` in a loop). Otherwise the arity is no
                    // longer known and the spread stays flagged.
                    return decl.kind == VariableDeclarationKind::Const
                        && decl
                            .init
                            .as_ref()
                            .is_some_and(|init| expression_is_statically_bounded_array(init, semantic))
                        && scoping
                            .get_resolved_references(sym_id)
                            .all(|r| !reference_is_array_growth_receiver(r.node_id(), semantic));
                }
            }
            false
        }

        _ => false,
    }
}

/// True when the reference at `ref_node_id` is the receiver of an in-place array
/// **growth** method call — `arr.push(...)`, `arr.unshift(...)`, `arr.splice(...)`
/// — which can add elements beyond the binding's initial arity. Any such call
/// makes a literal-initialized binding no longer statically bounded.
fn reference_is_array_growth_receiver(
    ref_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;

    let nodes = semantic.nodes();
    let ref_span = nodes.get_node(ref_node_id).kind().span();
    let AstKind::StaticMemberExpression(member) = nodes.kind(nodes.parent_id(ref_node_id)) else {
        return false;
    };
    if member.object.span() != ref_span {
        return false;
    }
    matches!(member.property.name.as_str(), "push" | "unshift" | "splice")
}

/// The module specifier `id` is imported from, if its binding resolves to an
/// `import ... from '<source>'` declaration. Returns `None` for an unresolved
/// reference or a binding that is not an import (e.g. a local function/param or a
/// hook-initialized `const`), so a same-named non-import binding is never
/// mistaken for the imported one.
///
/// Resolves the reference via `reference_id` → symbol → declaration node, then
/// walks the declaration node and its ancestors for the enclosing
/// `ImportDeclaration`. Callers apply their own predicate (exact match or a scope
/// prefix like `@react-navigation/`) to the returned source.
#[must_use]
pub fn import_source_of<'a>(
    id: &oxc_ast::ast::IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    use oxc_ast::AstKind;

    let ref_id = id.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
        .find_map(|kind| match kind {
            AstKind::ImportDeclaration(import) => Some(import.source.value.as_str()),
            _ => None,
        })
}

/// True when `id` resolves (via its binding) to an import whose source module
/// is one of `modules`. Returns `false` for an unresolved reference or a binding
/// that is not an import (e.g. a local function/param shadowing the name), so a
/// same-named non-import call is never mistaken for the imported one.
#[must_use]
pub fn resolves_to_import_from(
    id: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
    modules: &[&str],
) -> bool {
    import_source_of(id, semantic).is_some_and(|source| modules.contains(&source))
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

/// The PostgreSQL/libpq `sslmode` value that means "establish TLS but skip
/// certificate verification". A database driver honoring `?sslmode=no-verify`
/// must translate it to `{ rejectUnauthorized: false }`; that mapping carries
/// out the user's explicit, configurable opt-out, not a hardcoded insecure
/// default, so the TLS rules must not flag it.
const SSLMODE_NO_VERIFY: &str = "no-verify";

/// True when the node sits inside the branch taken when `sslmode=no-verify`
/// matched, i.e. a `switch`'s `case 'no-verify':` arm, or the consequent (the
/// `=== 'no-verify'` is-true branch) of an `if`/ternary whose test compares
/// (`===`/`==`) some value against the string literal `'no-verify'`.
///
/// This is the database-driver pattern from node-postgres: the disabling literal
/// only runs when the caller explicitly requested `sslmode=no-verify`. A bare
/// `{ rejectUnauthorized: false }` outside such a branch, or a branch keyed on
/// any other string, is unaffected and stays flagged — the carve-out keys on the
/// exact, specified sslmode value, not on the presence of any conditional.
///
/// Walks up the ancestor chain via the semantic node tree; the property/object is
/// confirmed to live in the matching branch's body by span containment.
#[must_use]
pub fn is_in_sslmode_no_verify_branch(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;

    let node_span = node.kind().span();
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::SwitchStatement(switch) => {
                for case in &switch.cases {
                    let Some(test) = &case.test else { continue };
                    if !is_no_verify_string_literal(test) {
                        continue;
                    }
                    if case
                        .consequent
                        .iter()
                        .any(|stmt| span_contains(stmt.span(), node_span))
                    {
                        return true;
                    }
                }
            }
            AstKind::IfStatement(if_stmt) => {
                if test_compares_no_verify(&if_stmt.test)
                    && span_contains(if_stmt.consequent.span(), node_span)
                {
                    return true;
                }
            }
            AstKind::ConditionalExpression(cond) => {
                if test_compares_no_verify(&cond.test)
                    && span_contains(cond.consequent.span(), node_span)
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Playwright/Puppeteer methods that serialize a function argument and execute
/// it inside a controlled automation browser context, not the application DOM.
const BROWSER_INJECTION_METHODS: &[&str] = &[
    "evaluate",
    "evaluateHandle",
    "evaluateOnNewDocument",
    "addInitScript",
    "$eval",
    "$$eval",
];

/// True when `node` sits inside a function/arrow that is a direct argument of a
/// browser-injection call — `page.evaluate(() => { document.write(html) })`,
/// `frame.addInitScript(...)`, `page.$eval(...)`, etc. The callback is
/// serialized and run in the automation browser, so DOM writes there target a
/// controlled automation page, not the application's XSS sink.
///
/// Walks ancestors from `node` to the nearest enclosing function and checks that
/// the function is a direct argument of a `CallExpression` whose callee is a
/// member named by [`BROWSER_INJECTION_METHODS`].
#[must_use]
pub(crate) fn is_inside_browser_injection_callback(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;

    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        if matches!(
            ancestor.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) && let AstKind::CallExpression(call) = nodes.parent_node(ancestor.id()).kind()
            && let Expression::StaticMemberExpression(member) = &call.callee
            && BROWSER_INJECTION_METHODS.contains(&member.property.name.as_str())
        {
            return true;
        }
    }
    false
}

/// True when `node` sits inside a function/arrow that is an argument of an
/// `.onError(...)` call — `.onError(({ set }) => { set.status = 500 })` or the
/// scope-qualified `.onError({ as: 'global' }, ({ set }) => { ... })`.
///
/// Inside an Elysia `.onError` handler the idiomatic shape is to mutate
/// `set.status` (and `set.headers`) separately while returning a computed body;
/// the `status(code, body)` helper is for route handlers, not the error
/// callback. So `set.status = ...` there is not a violation of
/// `elysia-prefer-status-over-set`.
///
/// Walks ancestors from `node` to each enclosing function and checks that the
/// function is a direct argument of a `CallExpression` whose callee is a member
/// named `onError`. Argument position is not constrained, so both the
/// single-argument and the scope-qualified two-argument forms are covered.
#[must_use]
pub(crate) fn is_inside_onerror_callback(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;

    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        if matches!(
            ancestor.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) && let AstKind::CallExpression(call) = nodes.parent_node(ancestor.id()).kind()
            && let Expression::StaticMemberExpression(member) = &call.callee
            && member.property.name.as_str() == "onError"
        {
            return true;
        }
    }
    false
}

/// True when `span` fully encloses `inner`.
pub(crate) fn span_contains(span: oxc_span::Span, inner: oxc_span::Span) -> bool {
    span.start <= inner.start && inner.end <= span.end
}

/// True when `expr` is the string literal `'no-verify'`.
fn is_no_verify_string_literal(expr: &oxc_ast::ast::Expression) -> bool {
    matches!(expr, oxc_ast::ast::Expression::StringLiteral(s) if s.value.as_str() == SSLMODE_NO_VERIFY)
}

/// True when `test` is an equality comparison (`===`/`==`, either operand order)
/// against the string literal `'no-verify'`.
fn test_compares_no_verify(test: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::BinaryOperator;

    let oxc_ast::ast::Expression::BinaryExpression(bin) = test else {
        return false;
    };
    if !matches!(
        bin.operator,
        BinaryOperator::StrictEquality | BinaryOperator::Equality
    ) {
        return false;
    }
    is_no_verify_string_literal(&bin.left) || is_no_verify_string_literal(&bin.right)
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

/// True when `ident` resolves to the **node parameter** (first formal parameter)
/// of a unist/unified tree-visitor callback — the function handed to `visit(...)`
/// or `visitParents(...)` from `unist-util-visit`. In the unified ecosystem
/// (remark/rehype, mdast/hast) transforming the tree is performed by mutating the
/// visited node in place (`node.type = 'html'`, `node.value = …`, `node.children
/// = …`); the visitor contract exposes no immutable return-a-new-node channel, so
/// there is nothing to suggest.
///
/// Structural anchors, resolved through the binding's declaration — never a `node`
/// name match:
/// - the binding is the **first formal parameter** of a function/arrow `F`, and
/// - `F` is the visitor of a `visit(...)`/`visitParents(...)` call, either inline
///   (a direct argument of that call) or by reference (a named function whose
///   identifier is passed as an argument to such a call elsewhere in the file).
///
/// An ordinary parameter mutated outside any visitor, and a non-first-parameter
/// local mutated inside one, both stay flagged.
#[must_use]
pub fn is_unist_visitor_node_param(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
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

    // Walk from the binding's declaration up to the function it parameterises;
    // require the binding to be that function's first formal parameter, then
    // confirm the function is registered as a unist visitor.
    let mut is_first_param = false;
    for ancestor in nodes.ancestors(decl_node_id) {
        match ancestor.kind() {
            AstKind::FormalParameters(params) => {
                is_first_param = params.items.first().is_some_and(|first| {
                    first.span.start <= decl_span.start && decl_span.end <= first.span.end
                });
                if !is_first_param {
                    return false;
                }
            }
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                if !is_first_param {
                    return false;
                }
                return fn_is_unist_visitor(ancestor, semantic);
            }
            _ => {}
        }
    }
    false
}

/// True when the callee identifies a `unist-util-visit` traversal entry point —
/// the free functions `visit` / `visitParents`, whose visitor-callback argument
/// mutates the handed-in node in place.
fn is_unist_visit_callee(callee: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;

    matches!(
        callee,
        Expression::Identifier(id) if matches!(id.name.as_str(), "visit" | "visitParents")
    )
}

/// True when the function/arrow node `func` is the visitor callback of a
/// `visit(...)`/`visitParents(...)` call — inline (a direct argument of the call)
/// or by reference (a named function declaration whose name is passed as an
/// argument to such a call anywhere in the file).
fn fn_is_unist_visitor(
    func: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;
    use oxc_span::GetSpan;

    let nodes = semantic.nodes();
    let fn_span = func.kind().span();

    // Inline: the callback is a direct argument of a visit/visitParents call
    // (arrow/function arguments carry no `Argument` wrapper, so the call is the
    // function node's parent).
    if let AstKind::CallExpression(call) = nodes.parent_node(func.id()).kind()
        && is_unist_visit_callee(&call.callee)
        && call
            .arguments
            .iter()
            .any(|arg| arg.as_expression().is_some_and(|e| e.span() == fn_span))
    {
        return true;
    }

    // By reference: the visitor is passed to a visit/visitParents call somewhere
    // in the file under a name (`visit(tree, 'code', visitor)`). Resolve that name
    // from either a function declaration's own id (`function visitor(node) {}`) or
    // the binding a function/arrow expression is assigned to (`const visitor =
    // (node) => {}`).
    let name = match func.kind() {
        AstKind::Function(decl) => decl.id.as_ref().map(|id| id.name.as_str()),
        _ => None,
    }
    .or_else(|| match nodes.parent_node(func.id()).kind() {
        AstKind::VariableDeclarator(decl) => {
            decl.id.get_binding_identifier().map(|bid| bid.name.as_str())
        }
        _ => None,
    });
    let Some(name) = name else {
        return false;
    };
    nodes.iter().any(|n| {
        let AstKind::CallExpression(call) = n.kind() else {
            return false;
        };
        is_unist_visit_callee(&call.callee)
            && call.arguments.iter().any(|arg| {
                matches!(
                    arg.as_expression(),
                    Some(Expression::Identifier(id)) if id.name.as_str() == name
                )
            })
    })
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

/// True when `ident` resolves to the Immer draft `state` of a Redux Toolkit
/// reducer, so mutating it (`state.x = …`, `state.list.push(…)`, `delete
/// state.x`) is the documented RTK pattern, not an aliased-state bug: Immer
/// records the draft mutation and produces a new immutable state, and there is
/// no spread/immutable form to suggest inside a reducer.
///
/// Two structural anchors, both resolved through the binding's declaration —
/// never a `state` name match:
/// - the binding is the **first formal parameter** of a function that is either
///   lexically nested under a `createSlice(...)` / `createReducer(...)` call
///   (the case-reducer callbacks, including the `creators.reducer((state) => …)`
///   builder form) or supplied directly to `builder.addCase/addMatcher/
///   addDefaultCase(...)`; or
/// - the binding (parameter or `const`/`let` declarator) is annotated with the
///   `Draft<…>` type from `immer` — the entity-adapter / query-slice helpers
///   take a `Draft<T>` state by reference.
///
/// An ordinary parameter mutated outside any reducer, and a non-first-param /
/// non-`Draft` local mutated inside one, both stay flagged.
#[must_use]
pub fn is_rtk_reducer_draft_param(
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

    if binding_is_immer_draft(decl_node_id, semantic) {
        return true;
    }

    // Otherwise require the binding to be the first formal parameter of a
    // function that is an RTK case reducer.
    if !decl_is_first_formal_parameter(decl_node_id, semantic) {
        return false;
    }
    for ancestor in nodes.ancestors(decl_node_id) {
        if matches!(
            ancestor.kind(),
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
        ) {
            return function_is_rtk_case_reducer(ancestor.id(), semantic);
        }
    }
    false
}

/// True when `decl_node_id` is the declaration of a function's first formal
/// parameter (the parameter chain reaches a `FormalParameters` whose first item
/// spans the declaration before any enclosing function boundary).
fn decl_is_first_formal_parameter(
    decl_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;
    let nodes = semantic.nodes();
    let decl_span = nodes.kind(decl_node_id).span();
    for ancestor in nodes.ancestors(decl_node_id) {
        match ancestor.kind() {
            AstKind::FormalParameters(params) => {
                return params.items.first().is_some_and(|first| {
                    first.span.start <= decl_span.start && decl_span.end <= first.span.end
                });
            }
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => return false,
            _ => {}
        }
    }
    false
}

/// True when the function node `fn_id` is a Redux Toolkit case reducer: it is
/// supplied directly to a `builder.addCase/addMatcher/addDefaultCase(...)` call,
/// or it is lexically nested anywhere inside the argument subtree of a
/// `createSlice(...)` / `createReducer(...)` call.
fn function_is_rtk_case_reducer(
    fn_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;
    let nodes = semantic.nodes();

    // Direct `builder.addCase(action, (state) => …)` argument.
    if let AstKind::CallExpression(call) = nodes.parent_node(fn_id).kind()
        && let Expression::StaticMemberExpression(member) = &call.callee
        && matches!(
            member.property.name.as_str(),
            "addCase" | "addMatcher" | "addDefaultCase"
        )
    {
        return true;
    }

    // Lexically nested under a createSlice / createReducer call.
    for ancestor in nodes.ancestors(fn_id) {
        if let AstKind::CallExpression(call) = ancestor.kind()
            && let Expression::Identifier(callee) = &call.callee
            && matches!(callee.name.as_str(), "createSlice" | "createReducer")
        {
            return true;
        }
    }
    false
}

/// True when the binding at `decl_node_id` (a parameter or variable declarator)
/// is annotated with the `Draft<…>` type imported from `immer` — Immer's branded
/// draft type, taken by reference by RTK entity-adapter / query-slice mutator
/// helpers. The `immer` import is required so a same-named domain `Draft<T>` is
/// not mistaken for it.
fn binding_is_immer_draft(
    decl_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    let nodes = semantic.nodes();
    let ann = match nodes.kind(decl_node_id) {
        AstKind::FormalParameter(param) => param.type_annotation.as_ref(),
        AstKind::VariableDeclarator(decl) => decl.type_annotation.as_ref(),
        _ => None,
    };
    ann.is_some_and(|ann| type_reference_name(&ann.type_annotation) == Some("Draft"))
        && file_imports_draft_from_immer(semantic)
}

/// True when the file has a `Draft` import specifier from `immer` (value or
/// `import type` form). Used to confirm a `Draft<…>` annotation is Immer's draft
/// type and not a same-named domain type.
fn file_imports_draft_from_immer(semantic: &oxc_semantic::Semantic) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::ImportDeclarationSpecifier;
    for node in semantic.nodes().iter() {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            continue;
        };
        if import.source.value.as_str() != "immer" {
            continue;
        }
        let Some(specifiers) = &import.specifiers else {
            continue;
        };
        for specifier in specifiers {
            if let ImportDeclarationSpecifier::ImportSpecifier(spec) = specifier
                && spec.imported.name().as_str() == "Draft"
            {
                return true;
            }
        }
    }
    false
}

/// Vue 3 ref factories whose return value is a `Ref<T>` wrapper mutated
/// through its `.value` property. `customRef` and (writable) `computed` follow
/// the same `ref.value = x` contract. Shared with the cross-file `ImportIndex`
/// extractor (`import_index.rs`) so a `.value` write on an imported ref is
/// recognized against the same factory set — the single source of truth.
pub(crate) const VUE_REF_FACTORIES: &[&str] = &["ref", "shallowRef", "customRef", "computed"];

/// Vue 3 reactive-object factory whose proxy converts every nesting level: a
/// nested object read back off the proxy is itself a proxy, so a property write at
/// any depth (`state.pageable.total = x`) drives reactivity.
const VUE_DEEP_REACTIVE_FACTORIES: &[&str] = &["reactive"];

/// Vue 3 reactive-object factory whose proxy tracks only its own root-level
/// properties — nested values are stored and exposed as-is, with no deep
/// conversion. Only a root-level write (`state.n = x`) drives reactivity; a nested
/// write reaches a raw object and is a plain-object mutation.
const VUE_SHALLOW_REACTIVE_FACTORIES: &[&str] = &["shallowReactive"];

/// Vue 3 *writable* ref-wrapper type-annotation names. A binding declared with
/// one of these holds a ref whose `.value` is the intended, assignable mutation
/// point, exactly like a locally-created `ref()` binding: the caller produced the
/// ref elsewhere, so `binding.value = x` is a reactive write, not a plain
/// object-property mutation. The read-only `ComputedRef` is excluded: a
/// `computed(getter)` value's `.value` is not assignable, so writing it is a
/// genuine error the rule keeps flagging. `WritableComputedRef` (from
/// `computed({ get, set })`) and `ModelRef` (from `defineModel`) are writable.
const VUE_WRITABLE_REF_TYPE_NAMES: &[&str] =
    &["Ref", "ShallowRef", "WritableComputedRef", "ModelRef"];

/// True when `ident` resolves to a `const`/`let` binding initialised by one of
/// `factories`, where the factory is Vue's. Resolves the binding via
/// `reference_id` → symbol → declaration node, then confirms the declarator
/// initializer is a call to one of the factory names whose callee is Vue's. Vue
/// origin holds either when the callee is a named import from `vue`
/// (`import { ref } from 'vue'`), or — in a project using `unplugin-auto-import`
/// — when the callee is a free/global identifier (auto-import injects
/// `ref`/`shallowRef`/… with no import statement) that resolves to no local
/// declaration. The no-local-declaration check keeps a same-named user-defined
/// local factory from being mistaken for Vue's.
fn is_vue_factory_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
    factories: &[&str],
    project: &crate::project::ProjectCtx,
    path: &Path,
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
            // Peel transparent wrappers: `reactive({…}) as PaginationProps` / `ref(0)!`
            // evaluate to the same factory-call value, so match the call shape through
            // them (see `peel_value_wrappers`).
            let init = peel_value_wrappers(init);
            let Expression::CallExpression(call) = init else {
                return false;
            };
            let Expression::Identifier(callee) = &call.callee else {
                return false;
            };
            return callee_is_vue_factory(callee, factories, semantic, project, path);
        }
    }
    false
}

/// True when `callee` names one of `factories` AND that name is Vue's — either a
/// named import from `vue` (`import { ref } from 'vue'`), or, in a project using
/// `unplugin-auto-import`, a free/global identifier that resolves to no local
/// declaration (auto-import injects `ref`/`computed`/… with no import statement).
/// The no-local-declaration check keeps a same-named user-defined factory from
/// being mistaken for Vue's.
fn callee_is_vue_factory(
    callee: &oxc_ast::ast::IdentifierReference,
    factories: &[&str],
    semantic: &oxc_semantic::Semantic,
    project: &crate::project::ProjectCtx,
    path: &Path,
) -> bool {
    let name = callee.name.as_str();
    factories.contains(&name)
        && (is_imported_from_vue(name, semantic)
            || (project.uses_unplugin_auto_import(path)
                && reference_resolves_to_no_local_binding(callee, semantic)))
}

/// True when `ident` is a free/global identifier — its reference resolves to no
/// declared symbol in any lexical scope. Distinguishes a real global (an
/// auto-injected Vue global like `shallowRef`, or the ECMAScript global object
/// `window`/`self`/`globalThis`) from a user-defined local of the same name, which
/// resolves to a binding.
pub fn reference_resolves_to_no_local_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return true;
    };
    semantic.scoping().get_reference(ref_id).symbol_id().is_none()
}

/// The type-name identifier of a *writable* Vue ref annotation — `Ref` for
/// `Ref<T>`, and likewise `ShallowRef`/`WritableComputedRef`/`ModelRef` (see
/// [`VUE_WRITABLE_REF_TYPE_NAMES`]). `None` for any other type, for the read-only
/// `ComputedRef`, and for a qualified (`Vue.Ref`) or `this`-qualified name: the
/// exemption is restricted to a bare ref-wrapper reference whose import
/// provenance a caller can resolve.
fn writable_vue_ref_type_ident<'a>(
    ty: &'a oxc_ast::ast::TSType<'a>,
) -> Option<&'a oxc_ast::ast::IdentifierReference<'a>> {
    use oxc_ast::ast::{TSType, TSTypeName};
    let TSType::TSTypeReference(tref) = ty else {
        return None;
    };
    let TSTypeName::IdentifierReference(id) = &tref.type_name else {
        return None;
    };
    VUE_WRITABLE_REF_TYPE_NAMES
        .contains(&id.name.as_str())
        .then_some(id.as_ref())
}

/// True when `ident` resolves to a binding whose declared type is a writable Vue
/// ref (see [`is_writable_vue_ref_type`] and [`binding_declared_ts_type`]): a
/// `Ref`/`ShallowRef`/`WritableComputedRef`/`ModelRef`-typed function parameter
/// (`x: Ref<T>`), annotated variable (`const x: Ref<T> = …`), or parameter
/// destructured from a same-file interface/type whose matching member carries
/// that type (`{ x }: Ctx` where `interface Ctx { x: Ref<T> }`). The caller
/// produced the ref, so `binding.value = x` is a reactive write, not a
/// plain-object mutation.
fn binding_is_writable_vue_ref_typed(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    binding_declared_ts_type(ident, semantic)
        .is_some_and(|ty| is_writable_vue_ref_type(ty, semantic))
}

/// True when `ty` is a writable Vue ref type whose type name is Vue's: a
/// `Ref`/`ShallowRef`/`WritableComputedRef`/`ModelRef` reference (see
/// [`writable_vue_ref_type_ident`]) whose name is imported from `vue`, or is a
/// bare/ambient name resolving to no other import (Vue's globally-declared or
/// `unplugin-auto-import`-injected ref types). A name imported from a non-`vue`
/// module is rejected, so a look-alike `Ref` from another package stays flagged.
fn is_writable_vue_ref_type(
    ty: &oxc_ast::ast::TSType,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    writable_vue_ref_type_ident(ty).is_some_and(|id| {
        type_ident_import_source(id, semantic).is_none_or(|module| module == "vue")
    })
}

/// The module a type-position identifier is imported from (`import { Ref } from
/// 'vue'` → `"vue"`), or `None` when it resolves to no import — an ambient/global
/// type, an auto-imported name, or a same-file declaration. Resolves the specific
/// reference to its binding (reference → symbol → declaration), so a same-named
/// local never masks the import and vice versa.
fn type_ident_import_source<'a>(
    id: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    use oxc_ast::AstKind;
    let ref_id = id.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
        .find_map(|kind| match kind {
            AstKind::ImportDeclaration(import) => Some(import.source.value.as_str()),
            _ => None,
        })
}

/// The effective declared TypeScript type of the binding `ident` resolves to,
/// across the shapes whose declaration carries a type: a directly-annotated
/// function parameter (`x: Ref<T>`), an annotated variable (`const x: Ref<T> = …`),
/// and a binding destructured from a typed object pattern — either an inline type
/// literal (`{ x }: { x: T }`) whose member supplies `x`'s type, or a named type
/// (`{ x }: Ctx`) whose same-file `interface`/`type` `Ctx` supplies member `x`'s
/// type. A named-type receiver is resolved by NAME regardless of its type arguments
/// (`{ x }: Ctx<T>`), since the member's type does not depend on them. Returns
/// `None` when the binding has no resolvable declared type (an inferred binding, an
/// un-annotated pattern, or an unresolvable member).
pub fn binding_declared_ts_type<'a>(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<&'a oxc_ast::ast::TSType<'a>> {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{BindingPattern, TSType};
    let ref_id = ident.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    // The binding's declaration is its parameter / variable-declarator (directly,
    // for a simple `x: T`) or nested under one (for a destructured `{ x }: Ctx`),
    // so it is the declaration node itself or the nearest such ancestor.
    let (pattern, annotation) = std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
        .find_map(|kind| match kind {
            AstKind::FormalParameter(param) => {
                Some((&param.pattern, param.type_annotation.as_ref()))
            }
            AstKind::VariableDeclarator(decl) => Some((&decl.id, decl.type_annotation.as_ref())),
            _ => None,
        })?;
    match pattern {
        // Simple `x: T` — the annotation is the binding's own type.
        BindingPattern::BindingIdentifier(_) => annotation.map(|ann| &ann.type_annotation),
        // Destructured `{ x }: T` (or renamed `{ k: x }: T`) — the member `x`
        // (resp. `k`), read from an inline type literal or a same-file named
        // `interface`/`type`.
        BindingPattern::ObjectPattern(obj) => {
            let annotation = &annotation?.type_annotation;
            let key = object_pattern_key_for_symbol(obj, sym_id)?;
            match annotation {
                TSType::TSTypeLiteral(lit) => signature_member_type(&lit.members, key),
                _ => named_type_member_type(type_reference_name(annotation)?, key, semantic, 0),
            }
        }
        _ => None,
    }
}

/// The object-pattern property key the destructured binding `sym_id` takes — the
/// member name to look up on the pattern's type. For a shorthand `{ x }` the key
/// is `x`; for a rename `{ k: x }` it is `k` (the object's member), not the local
/// `x`. `None` when the binding is not a direct (optionally defaulted) property.
fn object_pattern_key_for_symbol<'a>(
    obj: &'a oxc_ast::ast::ObjectPattern<'a>,
    sym_id: oxc_semantic::SymbolId,
) -> Option<&'a str> {
    obj.properties.iter().find_map(|prop| {
        (binding_pattern_leaf_symbol(&prop.value) == Some(sym_id))
            .then(|| property_key_name(&prop.key))
            .flatten()
    })
}

/// The symbol a simple (optionally defaulted) binding pattern binds:
/// `BindingIdentifier` directly, or the left of an `AssignmentPattern` default
/// (`{ x = d }`). `None` for a nested array/object pattern.
fn binding_pattern_leaf_symbol(
    pat: &oxc_ast::ast::BindingPattern,
) -> Option<oxc_semantic::SymbolId> {
    use oxc_ast::ast::BindingPattern;
    match pat {
        BindingPattern::BindingIdentifier(id) => id.symbol_id.get(),
        BindingPattern::AssignmentPattern(assign) => binding_pattern_leaf_symbol(&assign.left),
        _ => None,
    }
}

/// The static string name of a non-computed property key (`StaticIdentifier` or
/// `StringLiteral`), or `None` for a computed/numeric/other key.
fn property_key_name<'a>(key: &'a oxc_ast::ast::PropertyKey<'a>) -> Option<&'a str> {
    use oxc_ast::ast::PropertyKey;
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// The declared type of member `member` on the same-file `interface`/object-`type`
/// named `type_name`, following `extends` heritage and `type X = Y` aliases by name
/// up to [`OPTIONAL_MEMBER_RESOLUTION_DEPTH`] hops. Mirrors
/// [`named_type_has_optional_property`] but yields the member's type. Returns
/// `None` for an unknown type name, an absent member, or an exhausted depth budget.
fn named_type_member_type<'a>(
    type_name: &str,
    member: &str,
    semantic: &oxc_semantic::Semantic<'a>,
    depth: u32,
) -> Option<&'a oxc_ast::ast::TSType<'a>> {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Expression, TSType, TSTypeName};
    if depth >= OPTIONAL_MEMBER_RESOLUTION_DEPTH {
        return None;
    }
    for node in semantic.nodes().iter() {
        match node.kind() {
            AstKind::TSInterfaceDeclaration(decl) if decl.id.name.as_str() == type_name => {
                if let Some(ty) = signature_member_type(&decl.body.body, member) {
                    return Some(ty);
                }
                for heritage in &decl.extends {
                    if let Expression::Identifier(base) = &heritage.expression
                        && let Some(ty) =
                            named_type_member_type(base.name.as_str(), member, semantic, depth + 1)
                    {
                        return Some(ty);
                    }
                }
            }
            AstKind::TSTypeAliasDeclaration(decl) if decl.id.name.as_str() == type_name => {
                match &decl.type_annotation {
                    TSType::TSTypeLiteral(lit) => {
                        if let Some(ty) = signature_member_type(&lit.members, member) {
                            return Some(ty);
                        }
                    }
                    // A `type X = Y` alias to another named type — follow it.
                    TSType::TSTypeReference(tref) => {
                        if let TSTypeName::IdentifierReference(id) = &tref.type_name
                            && let Some(ty) =
                                named_type_member_type(id.name.as_str(), member, semantic, depth + 1)
                        {
                            return Some(ty);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    None
}

/// The declared type of the non-computed property signature named `member` in
/// `signatures` (`member: T`), or `None` when absent or untyped.
fn signature_member_type<'a>(
    signatures: &'a [oxc_ast::ast::TSSignature<'a>],
    member: &str,
) -> Option<&'a oxc_ast::ast::TSType<'a>> {
    use oxc_ast::ast::{PropertyKey, TSSignature};
    signatures.iter().find_map(|sig| {
        let TSSignature::TSPropertySignature(p) = sig else {
            return None;
        };
        let key_matches = !p.computed
            && match &p.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str() == member,
                PropertyKey::StringLiteral(s) => s.value.as_str() == member,
                _ => false,
            };
        key_matches
            .then(|| p.type_annotation.as_ref().map(|ann| &ann.type_annotation))
            .flatten()
    })
}

/// True when `ident` resolves to an annotation-less function parameter whose
/// default initializer is a Vue ref factory call (`queryClicks = ref(0)`). Such a
/// parameter holds a `Ref<T>` regardless of the caller's argument; its `.value` is
/// the reactive-update point. The default-init match requires a Vue factory (see
/// [`callee_is_vue_factory`]), so a plain default (`x = {}`) does not qualify.
fn binding_default_inits_vue_ref_factory(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
    project: &crate::project::ProjectCtx,
    path: &Path,
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
    let AstKind::FormalParameter(param) = semantic.nodes().kind(scoping.symbol_declaration(sym_id))
    else {
        return false;
    };
    if let Some(init) = &param.initializer
        && let Expression::CallExpression(call) = &**init
        && let Expression::Identifier(callee) = &call.callee
    {
        return callee_is_vue_factory(callee, VUE_REF_FACTORIES, semantic, project, path);
    }
    false
}

/// True when `ident` is imported from another project module whose matching
/// export binds a Vue ref factory call (`export const x = ref()/shallowRef()/
/// customRef()/computed()`). Confirms `ident` actually resolves to an import
/// binding (so a same-named local that shadows the import is not treated as the
/// import), then resolves its source module and original export name via the
/// [`ImportIndex`](crate::project::ImportIndex) and checks that module's export
/// table — following one `export { name } from './origin'` re-export hop — for a
/// ref-factory binding. A binding that does not resolve to a known exporting
/// file, or resolves to a non-ref-factory export, does not match. Resolution is
/// purely structural (binding provenance + import records + the exporting
/// module's declaration shape).
fn binding_is_imported_vue_ref(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
    project: &crate::project::ProjectCtx,
    path: &Path,
) -> bool {
    if !binding_resolves_to_import(ident, semantic) {
        return false;
    }
    let name = ident.name.as_str();
    let index = project.import_index();
    if index.is_empty() {
        return false;
    }
    let canon = index.canonical(path);
    index.get_imports(&canon).iter().any(|imp| {
        imp.local_name == name
            && imp
                .source_path
                .as_deref()
                .is_some_and(|src| export_is_vue_ref_factory(index, src, &imp.imported_name))
    })
}

/// True when `ident` resolves to a binding introduced by an `import` declaration
/// (its declaration node is, or is nested under, an `ImportDeclaration`). Keeps a
/// same-named local binding that shadows an import from being resolved through
/// the import records.
fn binding_resolves_to_import(
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
    std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
        .any(|kind| matches!(kind, AstKind::ImportDeclaration(_)))
}

/// True when `file` exports `name` as a Vue ref factory binding, directly or
/// through a single `export { name } from './origin'` re-export hop (the
/// centralized-state-module + barrel pattern common in Vue codebases).
fn export_is_vue_ref_factory(
    index: &crate::project::ImportIndex,
    file: &Path,
    name: &str,
) -> bool {
    let exports_ref_factory =
        |f: &Path| index.get_exports(f).iter().any(|e| e.name == name && e.is_vue_ref_factory);
    exports_ref_factory(file)
        || index.reexport_target(file, name).is_some_and(exports_ref_factory)
}

/// True when `ident` denotes a Vue `Ref<T>` wrapper whose `.value` property is
/// the *intended* mutation point: assigning `count.value = x` / `count.value++`
/// drives Vue's reactivity, not a plain-object mutation. Recognized when `ident`:
/// resolves to a `const`/`let` binding initialised by a Vue ref factory —
/// `ref(...)`, `shallowRef(...)`, `customRef(...)`, or `computed(...)` (imported
/// from `vue` or auto-injected by `unplugin-auto-import`; see
/// [`is_vue_factory_binding`]); has a declared type that is a writable Vue ref —
/// `Ref`/`ShallowRef`/`WritableComputedRef`/`ModelRef` resolved to a `vue` import
/// — on a parameter, a variable, or a parameter destructured from a same-file
/// interface/type (see [`binding_is_writable_vue_ref_typed`]); is an
/// annotation-less parameter defaulting to a ref-factory call (see
/// [`binding_default_inits_vue_ref_factory`]); or is imported from a module that
/// exports a ref-factory binding (see [`binding_is_imported_vue_ref`]). Callers
/// gate the `.value` property specifically; any other property write on the ref
/// stays flagged.
#[must_use]
pub fn is_vue_ref_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
    project: &crate::project::ProjectCtx,
    path: &Path,
) -> bool {
    is_vue_factory_binding(ident, semantic, VUE_REF_FACTORIES, project, path)
        || binding_is_writable_vue_ref_typed(ident, semantic)
        || binding_default_inits_vue_ref_factory(ident, semantic, project, path)
        || binding_is_imported_vue_ref(ident, semantic, project, path)
}

/// True when `member` is a `<ref>.value` access where `<ref>` is a direct
/// identifier bound to a Vue ref factory (`ref`/`shallowRef`/`customRef`/
/// `computed`, imported from `vue` or auto-injected by `unplugin-auto-import`;
/// see [`is_vue_factory_binding`]). This is the idiomatic Vue 3 reactive-update
/// target: `count.value = x` / `count.value++`. Restricted to the `value`
/// property and a direct-identifier base, so `ref.config = x` and `a.b.value = x`
/// stay flagged.
#[must_use]
pub fn is_vue_ref_value_target(
    member: &oxc_ast::ast::StaticMemberExpression,
    semantic: &oxc_semantic::Semantic,
    project: &crate::project::ProjectCtx,
    path: &Path,
) -> bool {
    use oxc_ast::ast::Expression;

    if member.property.name.as_str() != "value" {
        return false;
    }
    let Expression::Identifier(base) = &member.object else {
        return false;
    };
    is_vue_ref_binding(base, semantic, project, path)
}

/// True when `member` is a `<ident>.value` access where `<ident>` is a local
/// binding initialised by a function CALL — the standard "composable returns a
/// `Ref<T>`" pattern, whether the ref is bound directly
/// (`const theme = useStorage(k, v); theme.value = x`) or destructured
/// (`const { error } = useThing(); error.value = x`). A `Ref<T>` is mutated only
/// through `.value` regardless of which composable produced it and of how the
/// binding was taken, so a binding holding a call result is conservatively
/// treated as a potential ref.
///
/// Two structural constraints bound the exemption: the property must be `value`
/// (`x.config = y` stays flagged), and the initialiser must be a call
/// (`const o = { value: 1 }; o.value = 2` stays flagged). The binding must also
/// be declared outside any function nested in that initialiser, so a callback
/// parameter (`const rows = xs.map((r) => { r.value = 1; })`) does not inherit
/// the enclosing declarator's call.
///
/// The trade-off is a false negative when a call returns a plain object whose
/// `value` is a real data field (`const c = getConfig(); c.value = 5`):
/// distinguishing it from a ref would take a callee-name allowlist, which no
/// hand-written composable would match.
#[must_use]
pub fn is_call_ref_value_target(
    member: &oxc_ast::ast::StaticMemberExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;

    if member.property.name.as_str() != "value" {
        return false;
    }
    let Expression::Identifier(base) = &member.object else {
        return false;
    };
    let Some(ref_id) = base.reference_id.get() else {
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
        match kind {
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = &decl.init else {
                    return false;
                };
                // `await` / `as T` / `!` all evaluate to the value the call
                // produced (see `peel_value_wrappers`).
                return matches!(peel_value_wrappers(init), Expression::CallExpression(_));
            }
            // A binding declared inside a function is that function's own — a
            // parameter or a local — not the call result an enclosing
            // declarator holds.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

/// The root [`IdentifierReference`](oxc_ast::ast::IdentifierReference) a
/// member-access chain hangs off — `s` for `s`, `s.a.b`, and `s.list[0].done`.
/// Descends static- and computed-member links only, so a chain rooted at any
/// other expression yields `None`: an intervening call (`getState().a.b`), a
/// non-null assertion, or `this` produces a value whose declaration a caller
/// cannot resolve, and must not inherit the root binding's identity.
#[must_use]
pub fn root_identifier_of_expr<'a>(
    expr: &'a oxc_ast::ast::Expression<'a>,
) -> Option<&'a oxc_ast::ast::IdentifierReference<'a>> {
    use oxc_ast::ast::Expression;

    let mut current = expr;
    loop {
        current = match current {
            Expression::Identifier(id) => return Some(id),
            Expression::StaticMemberExpression(m) => &m.object,
            Expression::ComputedMemberExpression(m) => &m.object,
            _ => return None,
        };
    }
}

/// True when `member` is a property-write target on a Vue reactive-object proxy —
/// a binding initialised by `reactive(...)`/`shallowReactive(...)` (imported from
/// `vue`, or auto-injected by `unplugin-auto-import`; see
/// [`is_vue_factory_binding`]). A reactive proxy's properties are the *intended*
/// mutation point: `state.n = x` and `state.incrementedTimes++` are how Vue 3
/// reactivity is driven, with no immutable alternative. The property name is
/// unrestricted, unlike [`is_vue_ref_value_target`], where only `.value` is the
/// reactive point.
///
/// How far the exemption follows the member chain mirrors each factory's own
/// reactivity depth. `reactive()` converts every nesting level, so a write at any
/// depth (`state.pageable.total = x`, `state.list[0].done = true`) is a reactive
/// write: its chain root is resolved through member links only (see
/// [`root_identifier_of_expr`]), so a chain broken by a call (`getState().a.b = x`)
/// resolves to no binding and stays flagged. `shallowReactive()` exposes nested
/// values as-is, so only a root-level write on a direct-identifier base is
/// reactive; a nested write reaches a raw object and stays flagged.
#[must_use]
pub fn is_vue_reactive_object_target(
    member: &oxc_ast::ast::StaticMemberExpression,
    semantic: &oxc_semantic::Semantic,
    project: &crate::project::ProjectCtx,
    path: &Path,
) -> bool {
    use oxc_ast::ast::Expression;

    if root_identifier_of_expr(&member.object).is_some_and(|base| {
        is_vue_factory_binding(base, semantic, VUE_DEEP_REACTIVE_FACTORIES, project, path)
    }) {
        return true;
    }
    matches!(
        &member.object,
        Expression::Identifier(base)
            if is_vue_factory_binding(base, semantic, VUE_SHALLOW_REACTIVE_FACTORIES, project, path)
    )
}

/// True when `ident` resolves to a `const`/`let` binding initialised by a call to
/// `proxy(...)` imported from `valtio`. valtio's `proxy()` returns a reactive
/// Proxy whose *direct mutation* is the entire public API: `state.n = x`,
/// `state.n++`, and deep writes like `state.nested.ticks++` are intercepted by the
/// proxy to drive reactivity — there is no immutable alternative. Resolves the
/// binding via `reference_id` → symbol → declaration node, then confirms the
/// declarator initializer is a `proxy(...)` call whose callee is imported from
/// `valtio` (so a same-named local `proxy` is not mistaken for it). `ident` is the
/// *root* identifier of the mutation's member chain, so deep proxy writes are
/// covered without restricting to a direct-identifier base.
#[must_use]
pub fn is_valtio_proxy_binding(
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
            // Peel transparent wrappers: `proxy({…})!`, `proxy({…}) as Store` evaluate
            // to the same proxy value (see `peel_value_wrappers`).
            let init = peel_value_wrappers(init);
            let Expression::CallExpression(call) = init else {
                return false;
            };
            let Expression::Identifier(callee) = &call.callee else {
                return false;
            };
            return callee.name.as_str() == "proxy"
                && is_imported_from_valtio(callee.name.as_str(), semantic);
        }
    }
    false
}

/// True when `local_name` is the local binding of a named import from `valtio`
/// (`import { proxy } from 'valtio'`). Used to confirm a `proxy(...)` initializer
/// is valtio's reactive-proxy factory and not a same-named local function.
fn is_imported_from_valtio(local_name: &str, semantic: &oxc_semantic::Semantic) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::ImportDeclarationSpecifier;

    semantic.nodes().iter().any(|node| {
        let AstKind::ImportDeclaration(decl) = node.kind() else {
            return false;
        };
        if decl.source.value.as_str() != "valtio" {
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

/// True when `local_name` is the local binding of a named import from `vue`
/// (`import { ref } from 'vue'`).
#[must_use]
pub fn is_imported_from_vue(local_name: &str, semantic: &oxc_semantic::Semantic) -> bool {
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

/// True when `local_name` is the local binding of a named import from `react`
/// or `react-dom` (`import { use } from 'react'`). Used to confirm an
/// identifier resolves to a React export before applying a React-specific rule.
#[must_use]
pub fn is_imported_from_react(local_name: &str, semantic: &oxc_semantic::Semantic) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::ImportDeclarationSpecifier;

    semantic.nodes().iter().any(|node| {
        let AstKind::ImportDeclaration(decl) = node.kind() else {
            return false;
        };
        if !matches!(decl.source.value.as_str(), "react" | "react-dom") {
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

/// True when `assign` sets a `displayName` property as part of the React
/// DevTools naming convention rather than a state mutation. React reads
/// `displayName` off the component function object to name anonymous
/// `forwardRef`/`memo` results in DevTools, error messages, and stack traces — a
/// metadata API with no immutable alternative. The accepted RHS forms are:
/// - a string literal (`Component.displayName = "Component"`);
/// - a template literal (the HOC pattern `Component.displayName =
///   `Wrapped(${name})``); and
/// - another object's `.displayName` — property-name identity
///   `Foo.displayName = Primitive.Foo.displayName`, the Radix/shadcn wrapper
///   inherit pattern.
///
/// Any other RHS (e.g. a call like `Component.displayName = getName()`, or a
/// member access to a non-`displayName` property) stays flagged.
#[must_use]
pub fn is_react_display_name_assignment(assign: &oxc_ast::ast::AssignmentExpression) -> bool {
    use oxc_ast::ast::{AssignmentTarget, Expression};

    let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
        return false;
    };
    if member.property.name.as_str() != "displayName" {
        return false;
    }
    match &assign.right {
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => true,
        Expression::StaticMemberExpression(rhs) => rhs.property.name.as_str() == "displayName",
        _ => false,
    }
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

/// True when `operand` is an identifier whose binding is declared with a type
/// annotation that is a bare reference to a generic type parameter in scope at
/// `cast_node_id` (e.g. a parameter `serializedTransaction: serialized` where
/// `<serialized>` is the enclosing function's type parameter).
///
/// Casting such a value to a concrete type (`serializedTransaction as
/// TransactionSerializedCIP42`) cannot be replaced by a type guard: a predicate
/// `x is Concrete` narrows the *local* type, but TypeScript will not reduce the
/// generic type parameter itself, so the `as` is the only way to bridge the
/// generic to the concrete branch type. Parentheses are peeled.
#[must_use]
pub fn operand_is_typed_as_generic_param(
    operand: &oxc_ast::ast::Expression,
    cast_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{BindingPattern, Expression, TSType, TSTypeName};

    let Expression::Identifier(ident) = operand.without_parentheses() else {
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

    // Only trust the declaration's annotation when the binding is a bare
    // identifier (`x: T`). For a destructured binding (`{a}: T`, `[a]: T`) the
    // element's real type is `T["a"]`, not `T`, so casting it remains a genuine
    // narrowing.
    fn pattern_is_bare_identifier(pattern: &BindingPattern) -> bool {
        matches!(pattern, BindingPattern::BindingIdentifier(_))
    }

    let annotation = std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
        .find_map(|kind| match kind {
            AstKind::FormalParameter(param) if pattern_is_bare_identifier(&param.pattern) => {
                Some(param.type_annotation.as_ref())
            }
            AstKind::VariableDeclarator(decl) if pattern_is_bare_identifier(&decl.id) => {
                Some(decl.type_annotation.as_ref())
            }
            _ => None,
        });
    let Some(Some(annotation)) = annotation else {
        return false;
    };
    let TSType::TSTypeReference(r) = &annotation.type_annotation else {
        return false;
    };
    if r.type_arguments.is_some() {
        return false;
    }
    let TSTypeName::IdentifierReference(type_id) = &r.type_name else {
        return false;
    };
    name_is_generic_type_param_in_scope(type_id.name.as_str(), cast_node_id, semantic)
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

/// True when `node` is lexically inside a named function whose body is
/// serialized via `<fn>.toString()` and injected to run inside a browser realm
/// — the Playwright/Puppeteer "function-to-string" injection idiom:
///
/// ```ignore
/// function setupDragListeners() {
///   window.__cleanupDrag = () => { /* runs in the browser */ };
/// }
/// evaluateInAllFrames(`(${setupDragListeners.toString()})()`);
/// ```
///
/// The function is defined in the TypeScript source but its `.toString()` text
/// is concatenated into a script string (template literal, `'(' + fn.toString()
/// + ')'`, or `return` of such a string) and run in the page, where `window` is
/// the canonical global. `prefer-global-this` must stay silent on `window.*`
/// inside such a function, exactly as it does for a direct `*.evaluate(cb)`
/// callback ([`is_in_browser_eval_callback`]).
///
/// Detection is structural, not name-based: the nearest enclosing **named**
/// function (a `function f(){}` declaration, or a function/arrow expression bound
/// to a `const`/`let`/`var`) is resolved to its symbol, and the symbol's resolved
/// references are scanned for one used as the receiver of a `.toString()` call
/// (`f.toString()`). A plain top-level `window.foo`, or `window.*` inside a
/// function that is never `.toString()`-serialized, has no such reference and
/// stays flagged.
#[must_use]
pub fn is_inside_tostring_serialized_function<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_ast::AstKind;

    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let symbol_id = match ancestor.kind() {
            // `function f() { ... }` declaration uses its own binding name; an
            // anonymous function expression bound to a name (`const f =
            // function () { ... }`) falls back to the enclosing declarator.
            AstKind::Function(func) => func
                .id
                .as_ref()
                .and_then(|id| id.symbol_id.get())
                .or_else(|| enclosing_declarator_symbol(ancestor.id(), semantic)),
            // `const f = () => { ... }` — the enclosing variable declarator's
            // binding name.
            AstKind::ArrowFunctionExpression(_) => {
                enclosing_declarator_symbol(ancestor.id(), semantic)
            }
            _ => continue,
        };
        let Some(symbol_id) = symbol_id else {
            continue;
        };
        if symbol_has_tostring_call(symbol_id, semantic) {
            return true;
        }
    }
    false
}

/// Symbol of the `const`/`let`/`var` binding that the function/arrow expression
/// at `fn_node_id` is the *initializer* of, i.e. `const f = () => {}` → `f`.
/// `None` when the expression is not directly a declarator initializer (an inline
/// callback, an object-property value, …), since only a named binding can be
/// referenced by a later `.toString()` call.
fn enclosing_declarator_symbol(
    fn_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::SymbolId> {
    use oxc_ast::AstKind;
    use oxc_ast::ast::BindingPattern;
    use oxc_span::GetSpan;

    let nodes = semantic.nodes();
    let fn_span = nodes.get_node(fn_node_id).kind().span();
    let parent = nodes.get_node(nodes.parent_id(fn_node_id));
    let AstKind::VariableDeclarator(decl) = parent.kind() else {
        return None;
    };
    // The function must be the declarator's initializer, not nested deeper.
    if decl.init.as_ref().map(GetSpan::span) != Some(fn_span) {
        return None;
    }
    let BindingPattern::BindingIdentifier(id) = &decl.id else {
        return None;
    };
    id.symbol_id.get()
}

/// True when any resolved reference to `symbol_id` is the receiver of a
/// `.toString()` call — `f.toString()` for the function bound to `symbol_id`.
fn symbol_has_tostring_call(
    symbol_id: oxc_semantic::SymbolId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;

    let nodes = semantic.nodes();
    semantic
        .scoping()
        .get_resolved_references(symbol_id)
        .any(|reference| {
            let ref_span = nodes.get_node(reference.node_id()).kind().span();
            // The reference must be the *object* of a `.toString` member access
            // (`f.toString`) whose member is then called.
            let AstKind::StaticMemberExpression(member) =
                nodes.kind(nodes.parent_id(reference.node_id()))
            else {
                return false;
            };
            member.property.name.as_str() == "toString"
                && member.object.span() == ref_span
                && matches!(
                    nodes.kind(nodes.parent_id(nodes.parent_id(reference.node_id()))),
                    AstKind::CallExpression(_)
                )
        })
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

/// True when the reference to the global `name` at `node_id` is the operand of a
/// `typeof name` expression, or is lexically guarded by such an existence check
/// (`typeof name !== "undefined"` and the like). `typeof` never throws on an
/// undeclared global, so probing a global this way — then touching it only when
/// it exists — is the canonical cross-runtime feature-detection pattern, not the
/// implicit-global anti-pattern the runtime-portability rules target.
///
/// Two structural shapes are recognised:
/// - **operand**: `node_id` is the argument of a `typeof name` unary expression
///   (`typeof process !== "undefined"`);
/// - **guarded access**: `node_id` sits in the truthy branch of a guard whose
///   condition is *dominated* by a `typeof name` check —
///   `if (typeof name !== "undefined") { name.x }` (the consequent),
///   `typeof name !== "undefined" ? name.x : fallback` (the conditional's
///   consequent), or `typeof name !== "undefined" && name.x` (the `&&`'s
///   right operand). The check may sit inside an `&&`-chain in the condition
///   (`x && typeof name !== "undefined"`), since every conjunct must hold.
///
/// An access *outside* such a branch is not exempted: a bare `name.x` elsewhere
/// in the same file, the `else`/alternate branch, or a branch reachable via an
/// `||` arm that does not test `name` (`typeof name !== "undefined" || x`),
/// stays flagged. The check is purely structural over the AST, never a
/// name/path allowlist.
#[must_use]
pub fn is_typeof_existence_guarded(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
    name: &str,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;
    use oxc_span::GetSpan;

    let nodes = semantic.nodes();

    // (a) Direct operand of `typeof name` — `typeof process`. This is the
    // non-throwing existence probe, so it is never a real access. A member read
    // under a `typeof` (`typeof process.platform`) still evaluates `process`,
    // so it is intentionally NOT matched here and stays flagged unless a guard
    // covers it in branch (b).
    if matches!(
        nodes.kind(nodes.parent_id(node_id)),
        AstKind::UnaryExpression(unary)
            if unary.operator == oxc_ast::ast::UnaryOperator::Typeof
    ) {
        return true;
    }

    // (b) Guarded access: the node is in the truthy branch of a guard whose
    // condition feature-detects `name` via `typeof`.
    let mut child_span = nodes.get_node(node_id).kind().span();
    for ancestor in nodes.ancestors(node_id) {
        let test: &Expression = match ancestor.kind() {
            AstKind::IfStatement(stmt)
                if span_contains(stmt.consequent.span(), child_span) =>
            {
                &stmt.test
            }
            AstKind::ConditionalExpression(cond)
                if span_contains(cond.consequent.span(), child_span) =>
            {
                &cond.test
            }
            AstKind::LogicalExpression(logical)
                if logical.operator == oxc_ast::ast::LogicalOperator::And
                    && span_contains(logical.right.span(), child_span) =>
            {
                &logical.left
            }
            _ => {
                child_span = ancestor.kind().span();
                continue;
            }
        };
        if condition_guards_truthy_branch(test, name) {
            return true;
        }
        child_span = ancestor.kind().span();
    }
    false
}

/// True when a `typeof name` existence check in `expr` *dominates* the truthy
/// branch the condition guards — i.e. whenever the branch runs, `name` was
/// probed and found to exist.
///
/// Recurses only through positions that preserve that domination:
/// - the comparison itself (`typeof name !== "undefined"`, `typeof name ===
///   "object"`) — either operand may be the `typeof`;
/// - the left/right of a logical **AND** (`x && typeof name !== "undefined"`),
///   since both conjuncts must hold for the branch to run;
/// - a parenthesised sub-expression.
///
/// It deliberately does **not** descend into a logical **OR** or a negation:
/// in `typeof name !== "undefined" || x` the branch can run via `x` while
/// `name` is undefined, so the `typeof` does not guard it. Such an access stays
/// flagged.
fn condition_guards_truthy_branch(expr: &oxc_ast::ast::Expression, name: &str) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::UnaryExpression(unary) => {
            unary.operator == oxc_ast::ast::UnaryOperator::Typeof
                && matches!(&unary.argument, Expression::Identifier(id) if id.name == name)
        }
        Expression::BinaryExpression(bin) => {
            condition_guards_truthy_branch(&bin.left, name)
                || condition_guards_truthy_branch(&bin.right, name)
        }
        Expression::LogicalExpression(logical)
            if logical.operator == oxc_ast::ast::LogicalOperator::And =>
        {
            condition_guards_truthy_branch(&logical.left, name)
                || condition_guards_truthy_branch(&logical.right, name)
        }
        Expression::ParenthesizedExpression(paren) => {
            condition_guards_truthy_branch(&paren.expression, name)
        }
        _ => false,
    }
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
/// exempt on `implements`, which would introduce false negatives. `is_abstract`
/// (`abstract class`) marks a class designed to be subclassed, so its concrete
/// methods are virtual defaults that subclasses override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassShape {
    pub is_decorated: bool,
    pub has_super_class: bool,
    pub has_implements: bool,
    pub is_abstract: bool,
}

impl ClassShape {
    #[must_use]
    pub fn of(class: &oxc_ast::ast::Class) -> ClassShape {
        ClassShape {
            is_decorated: !class.decorators.is_empty(),
            has_super_class: class.super_class.is_some(),
            has_implements: !class.implements.is_empty(),
            is_abstract: class.r#abstract,
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

/// Built-in constructors that produce a brand-new, indexable, array-like value
/// with no prior alias: the dense-`Array` constructor plus every TypedArray
/// constructor. `new <name>(n)` is a freshly-created container, so a mutating
/// fill/sort/reverse chained directly onto it (`new Uint8Array(n).fill(0)`) is
/// unobservable through any other reference and must not be flagged as a
/// misleading in-place mutation.
const FRESH_ARRAY_CTORS: &[&str] = &[
    "Array",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float16Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
];

/// Whether `name` is a built-in constructor that yields a freshly-created
/// array-like value (the dense `Array` constructor or any TypedArray
/// constructor). Shared by the mutating-array rules so a `new <Ctor>(n)`
/// receiver is recognised as fresh in one place.
#[must_use]
pub fn is_fresh_array_ctor_name(name: &str) -> bool {
    FRESH_ARRAY_CTORS.contains(&name)
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

/// Peel transparent, value-preserving wrappers off `expr`, returning the first
/// inner expression that is none of them: parentheses, a `as T` / `<T>` /
/// `satisfies T` type cast, a `!` non-null assertion, or an `await`. Each
/// evaluates to the very object its operand produces, so a value-shape check (is
/// this a fresh array?) must see through arbitrary nesting of them —
/// `(await fetch()) as Item[]` holds the same array `fetch()` resolves to. Unlike
/// [`peel_parens`], this also strips the cast/`await` layers; the cast rules keep
/// `peel_parens` because they must still observe the `as`.
#[must_use]
fn peel_value_wrappers<'a>(
    expr: &'a oxc_ast::ast::Expression<'a>,
) -> &'a oxc_ast::ast::Expression<'a> {
    use oxc_ast::ast::Expression;
    let mut current = expr;
    loop {
        current = match current {
            Expression::ParenthesizedExpression(p) => &p.expression,
            Expression::TSAsExpression(a) => &a.expression,
            Expression::TSSatisfiesExpression(s) => &s.expression,
            Expression::TSNonNullExpression(n) => &n.expression,
            Expression::TSTypeAssertion(a) => &a.expression,
            Expression::AwaitExpression(a) => &a.argument,
            _ => return current,
        };
    }
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

/// External-library return types whose declared union admits both a bare
/// `return;` (yields `void`/`undefined`) and a value return, keyed by
/// `(source module, exported name)`. comply cannot parse the `.d.ts` in
/// `node_modules`, so such a type is recognized by import-graph provenance: the
/// return-type identifier's binding must resolve to a named import of that
/// exported name from that module. `NavigationGuardReturn` is vue-router's
/// navigation-guard return union (`void | boolean | RouteLocationRaw | …`), where
/// a bare `return;` continues navigation and `return false;`/a route cancels —
/// the same dual-return contract as Nuxt's `defineNuxtRouteMiddleware` callback.
const VOID_ADMITTING_IMPORTED_TYPES: &[(&str, &str)] = &[("vue-router", "NavigationGuardReturn")];

/// Maximum `type X = …` alias-resolution depth. Guards a pathological
/// self/mutually-referential alias chain (`type A = A | number`) against
/// unbounded recursion; genuine void-admitting aliases resolve in one or two hops.
const MAX_TYPE_ALIAS_DEPTH: u32 = 8;

/// True when `annotation` is a return-type that admits both `return;` (yields
/// `undefined`) and `return expr;`: a bare `void`/`undefined`/`any` keyword, a
/// union that includes any of them, a `Promise<T>` whose single awaited type
/// argument does, a same-file `type X = …` alias that resolves to any of the
/// above, or an external library type whose union admits `void` recognized by
/// import-graph provenance (see `VOID_ADMITTING_IMPORTED_TYPES`). `any` is a
/// superset of every type including `undefined`, so a bare `return;` is a valid
/// `any` value (the canonical `JSON.parse` reviver idiom returns `undefined` to
/// drop a key). In an `async` function a bare `return` resolves the promise to
/// `Promise.resolve(undefined)`, i.e. the `undefined`/`void` arm of a declared
/// `Promise<T | undefined>` / `Promise<T | void>`. Mixing bare and value returns
/// under such a contract is the canonical TypeScript idiom (e.g. `: any`,
/// `: T | undefined`, `: Promise<T | undefined>`, `: NavigationGuardReturn`, void
/// tail-calls), not an inconsistency.
#[must_use]
pub fn return_type_admits_void_or_undefined(
    annotation: Option<&oxc_ast::ast::TSTypeAnnotation>,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    annotation.is_some_and(|ann| ty_admits_void(&ann.type_annotation, semantic, 0))
}

/// Recursive core of `return_type_admits_void_or_undefined`. `depth` counts only
/// alias-resolution hops and is bounded by `MAX_TYPE_ALIAS_DEPTH`.
#[must_use]
fn ty_admits_void(ty: &oxc_ast::ast::TSType, semantic: &oxc_semantic::Semantic, depth: u32) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    match ty {
        TSType::TSVoidKeyword(_) | TSType::TSUndefinedKeyword(_) | TSType::TSAnyKeyword(_) => true,
        TSType::TSUnionType(union) => union
            .types
            .iter()
            .any(|member| ty_admits_void(member, semantic, depth)),
        TSType::TSTypeReference(tref) => {
            let TSTypeName::IdentifierReference(id) = &tref.type_name else {
                return false;
            };
            // `Promise<T>`: an async bare `return` resolves to
            // `Promise.resolve(undefined)`, so admit when the single awaited
            // type argument `T` does. Only the one-argument `Promise<T>` shape.
            if id.name == "Promise" {
                return tref.type_arguments.as_ref().is_some_and(|args| {
                    args.params.len() == 1
                        && args
                            .params
                            .first()
                            .is_some_and(|arg| ty_admits_void(arg, semantic, depth))
                });
            }
            // External library type recognized by import-graph provenance
            // (e.g. vue-router's `NavigationGuardReturn`): its `.d.ts` is
            // unreadable, so the binding must resolve to a named import of the
            // known export from the known module.
            if VOID_ADMITTING_IMPORTED_TYPES
                .iter()
                .any(|&(module, export)| type_ident_is_named_import_from(id, semantic, export, module))
            {
                return true;
            }
            // Same-file `type X = …` alias: recurse into the aliased type so any
            // local alias that unions in `void`/`undefined` is recognized.
            depth < MAX_TYPE_ALIAS_DEPTH
                && resolve_same_file_type_alias(id, semantic)
                    .is_some_and(|aliased| ty_admits_void(aliased, semantic, depth + 1))
        }
        _ => false,
    }
}

/// True when the type-position identifier `id` resolves (via its binding) to a
/// named import whose *exported* name is `export_name` and whose source module is
/// `module`. Binds the specific reference to its `ImportSpecifier`
/// (reference → symbol → declaration), so a same-named local type — or an import
/// of a different name or module — is never mistaken for it. Keys on the exported
/// name, so a renamed import (`import { NavigationGuardReturn as NGR }`) is still
/// recognized.
#[must_use]
fn type_ident_is_named_import_from(
    id: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
    export_name: &str,
    module: &str,
) -> bool {
    use oxc_ast::AstKind;
    let Some(ref_id) = id.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    let mut exported_name_matches = false;
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id)) {
        match kind {
            AstKind::ImportSpecifier(spec) => {
                exported_name_matches = spec.imported.name().as_str() == export_name;
            }
            AstKind::ImportDeclaration(import) => {
                return exported_name_matches && import.source.value.as_str() == module;
            }
            _ => {}
        }
    }
    false
}

/// The same-file `type X = …` alias *declaration* that the type-position
/// identifier `id` names, or `None` when its binding is not a local type-alias
/// declaration (an import, a type parameter, a value binding, or an unresolved
/// reference). Resolves the reference → symbol → declaration node. Exposes the
/// whole declaration so callers that need the generic parameter list (to
/// substitute call-site type-arguments) can reach it, not only the aliased type.
#[must_use]
fn resolve_same_file_type_alias_decl<'a>(
    id: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<&'a oxc_ast::ast::TSTypeAliasDeclaration<'a>> {
    use oxc_ast::AstKind;
    let ref_id = id.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    match semantic.nodes().kind(scoping.symbol_declaration(sym_id)) {
        AstKind::TSTypeAliasDeclaration(alias) => Some(alias),
        _ => None,
    }
}

/// The aliased `TSType` of the same-file `type X = …` alias that `id` names, or
/// `None` when its binding is not a local type-alias declaration.
#[must_use]
fn resolve_same_file_type_alias<'a>(
    id: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<&'a oxc_ast::ast::TSType<'a>> {
    resolve_same_file_type_alias_decl(id, semantic).map(|alias| &alias.type_annotation)
}

/// The runtime primitive-keyword types a branded / opaque primitive is built on
/// (`string & { __brand }`). `any`/`unknown`/`object`/`null`/`undefined`/`void`/
/// `never` are excluded: they are not the underlying carrier of a nominal brand.
#[must_use]
fn is_primitive_keyword(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::TSType;
    matches!(
        ty,
        TSType::TSStringKeyword(_)
            | TSType::TSNumberKeyword(_)
            | TSType::TSBooleanKeyword(_)
            | TSType::TSSymbolKeyword(_)
            | TSType::TSBigIntKeyword(_)
    )
}

/// True when the same-file `type X = …` alias named by `id` resolves to a
/// branded / opaque primitive: a `TSIntersectionType` that carries at least one
/// primitive-keyword member (`string`/`number`/`boolean`/`symbol`/`bigint`),
/// either directly (`type Brand = string & { __brand: 'x' }`) or through a
/// one-hop generic helper whose primitive type-argument substitutes into a
/// parameter position of its intersection (`type Opaque<K, T> = T & { __brand: K }`
/// instantiated as `type Key = Opaque<'Key', string>`).
///
/// A branded primitive is a nominal-typing phantom: `__brand` has no runtime
/// representation, so no `typeof`/`in`/type-predicate check can distinguish the
/// value from its underlying primitive. The `as` cast is the only mechanism
/// TypeScript offers to mint the brand, making it a construction-time ascription
/// rather than a refinement of a pre-existing binding.
///
/// Termination is by construction: at most two resolution hops are taken (the
/// named alias, then optionally one generic helper it references); nothing
/// recurses, so a self-referential alias cannot loop.
#[must_use]
pub fn resolves_to_branded_primitive(
    id: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    let Some(aliased) = resolve_same_file_type_alias(id, semantic) else {
        return false;
    };
    match aliased {
        // Direct brand: `type Brand = string & { __brand: 'x' }`.
        TSType::TSIntersectionType(intersection) => {
            intersection.types.iter().any(is_primitive_keyword)
        }
        // One-hop generic helper: `type Key = Opaque<'Key', string>` where
        // `Opaque<K, T> = T & { __brand: K }`. Resolve the helper and check its
        // intersection with the call-site type-arguments substituted in.
        TSType::TSTypeReference(tref) => {
            let TSTypeName::IdentifierReference(helper_id) = &tref.type_name else {
                return false;
            };
            resolve_same_file_type_alias_decl(helper_id, semantic).is_some_and(|helper| {
                helper_intersection_has_primitive(helper, tref.type_arguments.as_deref())
            })
        }
        _ => false,
    }
}

/// True when the generic helper alias `helper` is an intersection that carries a
/// primitive-keyword member once `type_args` substitute into its parameters —
/// either a member that is already a primitive keyword, or a bare reference to a
/// helper type parameter whose corresponding call-site type-argument is a
/// primitive keyword (`T` in `T & { __brand: K }` bound to `string`).
#[must_use]
fn helper_intersection_has_primitive(
    helper: &oxc_ast::ast::TSTypeAliasDeclaration,
    type_args: Option<&oxc_ast::ast::TSTypeParameterInstantiation>,
) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    let TSType::TSIntersectionType(intersection) = &helper.type_annotation else {
        return false;
    };
    let params = helper.type_parameters.as_deref();
    intersection.types.iter().any(|member| {
        if is_primitive_keyword(member) {
            return true;
        }
        let TSType::TSTypeReference(mref) = member else {
            return false;
        };
        if mref.type_arguments.is_some() {
            return false;
        }
        let TSTypeName::IdentifierReference(param_ref) = &mref.type_name else {
            return false;
        };
        substituted_type_argument(param_ref.name.as_str(), params, type_args)
            .is_some_and(is_primitive_keyword)
    })
}

/// The call-site type-argument bound to the helper type parameter named
/// `param_name`, or `None` when the name is not a declared parameter or no
/// argument occupies its position. Matches the parameter name to its index in
/// the declaration, then reads the same-index member of the instantiation.
#[must_use]
fn substituted_type_argument<'a>(
    param_name: &str,
    params: Option<&oxc_ast::ast::TSTypeParameterDeclaration>,
    type_args: Option<&'a oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
) -> Option<&'a oxc_ast::ast::TSType<'a>> {
    let index = params?
        .params
        .iter()
        .position(|p| p.name.name.as_str() == param_name)?;
    type_args?.params.get(index)
}

/// True when `ident` (a type-position identifier) resolves to a symbol whose
/// declaration node is a `TSTypeParameter` — i.e. a generic parameter bound to
/// an enclosing class, function, interface, or type-alias scope. Resolves the
/// reference via `reference_id` → symbol → declaration node, so a same-named
/// concrete type or value binding is correctly rejected.
#[must_use]
fn ident_resolves_to_type_parameter(
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
    let decl = scoping.symbol_declaration(sym_id);
    matches!(semantic.nodes().kind(decl), AstKind::TSTypeParameter(_))
}

/// True when `t` references at least one enclosing-scope type parameter, or the
/// polymorphic `this` type, looking through union/intersection members,
/// parentheses, generic type arguments, conditional types, indexed accesses,
/// array/tuple element types, and type operators (so `ReturnType<CallFunction>`,
/// `CTEBuilderCallback<N>`, `T extends A ? T['x'] : string`, `this['_']['data']`,
/// `T[]`, `keyof T`, and `[T, U]` are all seen to reference their parameter or
/// `this`). Such a type is instantiation-dependent: its concrete form is deferred
/// to whatever the parameter (or `this`) is bound to per call, so it cannot be
/// lifted to a module-level alias.
#[must_use]
pub fn type_references_enclosing_type_parameter(
    t: &oxc_ast::ast::TSType,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    match t {
        TSType::TSTypeReference(tref) => {
            if let TSTypeName::IdentifierReference(id) = &tref.type_name
                && ident_resolves_to_type_parameter(id, semantic)
            {
                return true;
            }
            tref.type_arguments.as_ref().is_some_and(|args| {
                args.params
                    .iter()
                    .any(|p| type_references_enclosing_type_parameter(p, semantic))
            })
        }
        TSType::TSUnionType(u) => u
            .types
            .iter()
            .any(|m| type_references_enclosing_type_parameter(m, semantic)),
        TSType::TSIntersectionType(i) => i
            .types
            .iter()
            .any(|m| type_references_enclosing_type_parameter(m, semantic)),
        // A conditional type (`T extends A ? B : C`) is instantiation-dependent
        // when its test, constraint, or either branch references an enclosing
        // type parameter (or `this`); recurse into all four so a fully-concrete
        // conditional still resolves to `false` and stays extractable.
        TSType::TSConditionalType(c) => {
            type_references_enclosing_type_parameter(&c.check_type, semantic)
                || type_references_enclosing_type_parameter(&c.extends_type, semantic)
                || type_references_enclosing_type_parameter(&c.true_type, semantic)
                || type_references_enclosing_type_parameter(&c.false_type, semantic)
        }
        // An indexed access (`this['_']['data']`, `T['data']`) inherits scope
        // dependence from its object or index type; recurse into both.
        TSType::TSIndexedAccessType(a) => {
            type_references_enclosing_type_parameter(&a.object_type, semantic)
                || type_references_enclosing_type_parameter(&a.index_type, semantic)
        }
        // The polymorphic `this` type is bound to its enclosing class/interface
        // scope and is invalid at module scope, so it can never be hoisted.
        TSType::TSThisType(_) => true,
        // Unwrap parentheses so the walk reaches the inner type of a
        // parenthesized member (`(T extends A ? B : C) | SQL`).
        TSType::TSParenthesizedType(p) => {
            type_references_enclosing_type_parameter(&p.type_annotation, semantic)
        }
        // Element-wrapping forms inherit scope dependence from the type they
        // wrap: `T[]` (array), `keyof T` / `readonly T[]` (type operator), and
        // `[T, U]` (tuple) each hold an enclosing type parameter that is equally
        // unhoistable, so recurse into the wrapped element type(s).
        TSType::TSArrayType(arr) => {
            type_references_enclosing_type_parameter(&arr.element_type, semantic)
        }
        TSType::TSTypeOperatorType(op) => {
            type_references_enclosing_type_parameter(&op.type_annotation, semantic)
        }
        TSType::TSTupleType(tuple) => tuple
            .element_types
            .iter()
            .any(|el| tuple_element_references_enclosing_type_parameter(el, semantic)),
        // A named tuple member (`[first: T]`) labels its element; the scope
        // dependence lives in the inner element type.
        TSType::TSNamedTupleMember(member) => {
            tuple_element_references_enclosing_type_parameter(&member.element_type, semantic)
        }
        _ => false,
    }
}

/// Recurse into a single tuple element, unwrapping optional (`[T?]`) and rest
/// (`[...T[]]`) markers, so an enclosing type parameter held anywhere inside a
/// tuple member is seen.
fn tuple_element_references_enclosing_type_parameter(
    el: &oxc_ast::ast::TSTupleElement,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::TSTupleElement;
    match el {
        TSTupleElement::TSOptionalType(opt) => {
            type_references_enclosing_type_parameter(&opt.type_annotation, semantic)
        }
        TSTupleElement::TSRestType(rest) => {
            type_references_enclosing_type_parameter(&rest.type_annotation, semantic)
        }
        other => other
            .as_ts_type()
            .is_some_and(|inner| type_references_enclosing_type_parameter(inner, semantic)),
    }
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
pub(crate) fn import_root_package(specifier: &str) -> &str {
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

/// True when `source` text mentions a quoted module specifier whose root package
/// is a known database / ORM library ([`DB_PACKAGES`]). A text-level counterpart
/// to [`file_imports_db_library`] for backends that have no parsed AST (the `.vue`
/// Text backend, whose `<script>` block is not parsed by oxc).
///
/// Matches the specifier inside any single- or double-quoted string, which covers
/// `import … from "pg"`, `require('pg')`, and dynamic `import("pg")` without
/// re-parsing. The quotes anchor the match to a real specifier, so `pg` inside an
/// identifier or prose never matches, and subpaths (`drizzle-orm/node-postgres`)
/// resolve to their root package.
#[must_use]
pub fn source_imports_db_library(source: &str) -> bool {
    quoted_specifiers(source).any(|spec| DB_PACKAGES.contains(&import_root_package(spec)))
}

/// Yields the contents of every single- or double-quoted string in `source`.
/// A crude scan with no escape handling — enough for matching import specifiers,
/// which never contain quotes.
fn quoted_specifiers(source: &str) -> impl Iterator<Item = &str> {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    std::iter::from_fn(move || {
        while cursor < bytes.len() {
            let quote = bytes[cursor];
            cursor += 1;
            if quote != b'"' && quote != b'\'' {
                continue;
            }
            let start = cursor;
            while cursor < bytes.len() && bytes[cursor] != quote && bytes[cursor] != b'\n' {
                cursor += 1;
            }
            if cursor < bytes.len() && bytes[cursor] == quote {
                let spec = &source[start..cursor];
                cursor += 1;
                return Some(spec);
            }
        }
        None
    })
}

/// Known HTML-email template libraries. Components built with these render to
/// email markup, where inline `style={{…}}` is the *only* reliable styling
/// mechanism — every major client (Outlook, Gmail, Apple Mail) strips `<style>`
/// blocks and external CSS. UI rules that push styles out of the markup
/// (e.g. `ui-no-inline-exhaustive-style`) must not fire on these files.
///
/// Matched against the *root* package of every import specifier, plus the
/// `@react-email/` scope (whose primitives live under several sibling packages
/// like `@react-email/button`).
const EMAIL_TEMPLATE_PACKAGES: &[&str] = &[
    "@react-email/components",
    "react-email",
    "jsx-email",
    "mjml",
    "mjml-react",
];

/// True when the file imports at least one known HTML-email template library
/// ([`EMAIL_TEMPLATE_PACKAGES`], plus any `@react-email/*` sub-package). Covers
/// static `import`/`export … from`, dynamic `import('…')`, and CommonJS
/// `require('…')`.
#[must_use]
pub fn file_imports_email_template_library(semantic: &oxc_semantic::Semantic<'_>) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Argument, Expression};

    let is_email_specifier = |spec: &str| {
        let root = import_root_package(spec);
        EMAIL_TEMPLATE_PACKAGES.contains(&root) || root.starts_with("@react-email/")
    };

    semantic.nodes().iter().any(|node| match node.kind() {
        AstKind::ImportDeclaration(decl) => is_email_specifier(decl.source.value.as_str()),
        AstKind::ExportNamedDeclaration(decl) => decl
            .source
            .as_ref()
            .is_some_and(|src| is_email_specifier(src.value.as_str())),
        AstKind::ExportAllDeclaration(decl) => is_email_specifier(decl.source.value.as_str()),
        AstKind::ImportExpression(expr) => {
            matches!(peel_parens(&expr.source), Expression::StringLiteral(lit)
                if is_email_specifier(lit.value.as_str()))
        }
        AstKind::CallExpression(call) => {
            let is_require = matches!(&call.callee, Expression::Identifier(id) if id.name == "require");
            is_require
                && matches!(call.arguments.first(), Some(Argument::StringLiteral(lit))
                    if is_email_specifier(lit.value.as_str()))
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

/// The TypedArray constructor family. A TypedArray is a fixed-length view over a
/// binary buffer; its elements are written exclusively through indexed assignment
/// (`buf[i] = v`) — there is no immutable element-setter and no spread-then-build
/// form (spreading a TypedArray yields a plain `Array`). Excludes `Array`, whose
/// element writes do have immutable alternatives.
const TYPED_ARRAY_CTORS: &[&str] = &[
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float16Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
];

/// True when `name` is a standard TypedArray constructor (`Uint8Array`,
/// `Int8Array`, `Float64Array`, …). The view over the [`TYPED_ARRAY_CTORS`]
/// family for callers that have the constructor name in hand rather than a
/// binding.
#[must_use]
pub fn is_typed_array_ctor_name(name: &str) -> bool {
    TYPED_ARRAY_CTORS.contains(&name)
}

/// True when `ident` resolves to a binding whose value is a TypedArray — a
/// fixed-length binary buffer (`Uint8Array`, `Float64Array`, `Int32Array`, …).
/// Indexed element assignment (`buf[i] = v`) is the only way to populate a
/// TypedArray's contents: spreading one produces a plain `Array`, and a sparse
/// lookup table can't be expressed as a constructor literal, so there is no
/// immutable element-setter to suggest. Mutation rules use this to exempt such
/// element writes while still flagging plain object/array property mutation.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, then
/// recognizes a TypedArray binding three ways on the enclosing
/// `VariableDeclarator`:
/// 1. a `: Uint8Array` (or other TypedArray) type annotation on the declarator;
/// 2. an initializer that produces a TypedArray — `new Uint8Array(...)`,
///    `Uint8Array.from(...)` / `Uint8Array.of(...)`, or a `.subarray(...)` /
///    `.slice(...)` call whose own receiver resolves to a TypedArray (a TypedArray
///    view of a TypedArray).
///
/// A plain `Array` binding, a function parameter, an import, or any other shape
/// resolves to no TypedArray signal and returns `false`, so mutation through it
/// stays flagged.
#[must_use]
pub fn is_typed_array_binding(
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
            if decl
                .type_annotation
                .as_ref()
                .is_some_and(|ann| type_is_typed_array(&ann.type_annotation))
            {
                return true;
            }
            return decl
                .init
                .as_ref()
                .is_some_and(|init| expression_produces_typed_array(init, semantic));
        }
    }
    false
}

/// True when `ty` is a direct type reference naming a TypedArray
/// (`Uint8Array`, `Float64Array`, …). A union or aliased type does not qualify.
fn type_is_typed_array(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    let TSType::TSTypeReference(reference) = ty else {
        return false;
    };
    matches!(
        &reference.type_name,
        TSTypeName::IdentifierReference(id) if TYPED_ARRAY_CTORS.contains(&id.name.as_str())
    )
}

/// True when `expr` evaluates to a TypedArray: a `new <TypedArray>(...)`
/// construction, a `<TypedArray>.from(...)`/`.of(...)` static factory, or a
/// `.subarray(...)`/`.slice(...)` view whose receiver itself resolves to a
/// TypedArray. Looks through a trailing non-null assertion (`buf!`).
fn expression_produces_typed_array(
    expr: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::Expression;

    match expr {
        Expression::TSNonNullExpression(nn) => {
            expression_produces_typed_array(&nn.expression, semantic)
        }
        // `new Uint8Array(...)`
        Expression::NewExpression(new_expr) => matches!(
            &new_expr.callee,
            Expression::Identifier(callee) if TYPED_ARRAY_CTORS.contains(&callee.name.as_str())
        ),
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            match &member.object {
                // `Uint8Array.from(...)` / `Uint8Array.of(...)`
                Expression::Identifier(obj)
                    if TYPED_ARRAY_CTORS.contains(&obj.name.as_str()) =>
                {
                    matches!(member.property.name.as_str(), "from" | "of")
                }
                // `buf.subarray(...)` / `buf.slice(...)` — a TypedArray view of a
                // TypedArray. The receiver must itself resolve to a TypedArray
                // binding, so a `.slice()` on a plain array does not qualify.
                Expression::Identifier(obj)
                    if matches!(member.property.name.as_str(), "subarray" | "slice") =>
                {
                    is_typed_array_binding(obj, semantic)
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// True when `ident` resolves to a `const`-declared local array that was
/// initialised as an empty array literal — `const x = []`. Such a binding is a
/// locally-owned array being *built* into a sparse dispatch / lookup table by
/// constant-index assignment (`handlers[0x01] = fn`), the array analogue of the
/// `const items = []; items.push(x)` accumulator: the sparse layout can't be a
/// constructor literal, so indexed assignment is the only way to populate it and
/// there is no immutable element-setter to suggest.
///
/// Only the empty array *literal* `[]` qualifies — a populated literal (`[a, b]`)
/// is a complete array, and a pre-sized `new Array(n)` is a fixed-length buffer
/// whose plain-array element writes the rules deliberately keep flagged; later
/// indexed writes on either are mutation, not table construction. Resolution uses
/// the same `reference_id` → symbol → declaration path as [`is_typed_array_binding`]:
/// a function parameter, import, `let`/`var` binding, or non-`[]` initializer
/// resolves to no signal and returns `false`, so mutation through it stays flagged.
#[must_use]
pub fn is_local_dispatch_table_binding(
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
            return decl.kind == VariableDeclarationKind::Const
                && matches!(
                    &decl.init,
                    Some(Expression::ArrayExpression(arr)) if arr.elements.is_empty()
                );
        }
    }
    false
}

/// True when `index` is a constant/static dispatch-table key: a numeric literal
/// (`handlers[0x01]`) or an identifier resolving to a `const` binding (a named
/// opcode constant `handlers[messageSync]`). A dynamic index (a loop variable, a
/// `let`/parameter, or a computed expression) does not qualify, keeping the
/// dispatch-table exemption tight: only constant-keyed table construction is
/// exempt, not arbitrary runtime indexed writes.
#[must_use]
pub fn is_constant_index_expression(
    index: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Expression, VariableDeclarationKind};

    match index {
        Expression::NumericLiteral(_) => true,
        Expression::Identifier(id) => {
            let Some(ref_id) = id.reference_id.get() else {
                return false;
            };
            let scoping = semantic.scoping();
            let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
                return false;
            };
            let decl_node_id = scoping.symbol_declaration(sym_id);
            let nodes = semantic.nodes();
            for kind in std::iter::once(nodes.kind(decl_node_id))
                .chain(nodes.ancestor_kinds(decl_node_id))
            {
                match kind {
                    AstKind::VariableDeclaration(decl) => {
                        return decl.kind == VariableDeclarationKind::Const;
                    }
                    // A parameter / function / module scope reached before any
                    // `VariableDeclaration` means the index is not a `const`
                    // binding — stop here so a parameter index never resolves to
                    // an enclosing `const f = (k) => …` declarator.
                    AstKind::FormalParameter(_)
                    | AstKind::Function(_)
                    | AstKind::ArrowFunctionExpression(_)
                    | AstKind::Program(_) => {
                        return false;
                    }
                    _ => {}
                }
            }
            false
        }
        _ => false,
    }
}

/// True when the function/arrow node `func_id` is the direct callee of a call
/// expression — an IIFE (`(() => ...)()`). An IIFE runs immediately at its
/// definition site, so its body executes exactly once per execution of the
/// IIFE's own enclosing context rather than once per call of a reusable
/// function. Parenthesized wrappers around the callee are transparent: the
/// callee span is then the wrapper's, so the comparison tracks the span of the
/// child node just below each ancestor.
pub fn function_is_immediately_invoked(
    nodes: &oxc_semantic::AstNodes,
    func_id: oxc_semantic::NodeId,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;
    let mut child_span = nodes.kind(func_id).span();
    for ancestor_id in nodes.ancestor_ids(func_id) {
        match nodes.kind(ancestor_id) {
            AstKind::ParenthesizedExpression(_) => child_span = nodes.kind(ancestor_id).span(),
            AstKind::CallExpression(call) => return call.callee.span() == child_span,
            _ => return false,
        }
    }
    false
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
        file_imports_email_template_library, has_ts_expect_error_above, is_as_unknown_double_cast,
        is_outer_as_unknown_double_cast, node_has_preceding_deprecated_tag, peel_parens,
        source_imports_db_library, type_annotation_is_type_predicate, with_semantic,
    };
    use oxc_ast::AstKind;
    use oxc_span::SourceType;

    fn imports_db(src: &str) -> bool {
        with_semantic(src, SourceType::ts(), file_imports_db_library)
    }

    fn imports_email(src: &str) -> bool {
        with_semantic(src, SourceType::tsx(), file_imports_email_template_library)
    }

    #[test]
    fn file_imports_email_template_library_detects_known_packages() {
        assert!(imports_email("import { Button } from '@react-email/components';"));
        assert!(imports_email("import { Button } from '@react-email/button';"));
        assert!(imports_email("import { Button } from 'jsx-email';"));
        assert!(imports_email("import mjml from 'mjml';"));
    }

    #[test]
    fn file_imports_email_template_library_rejects_non_email_imports() {
        assert!(!imports_email("import React from 'react';"));
        assert!(!imports_email("import { foo } from './email';"));
        assert!(!imports_email("const x = 1;"));
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
    fn source_imports_db_library_detects_quoted_specifiers_and_subpaths() {
        assert!(source_imports_db_library("import { drizzle } from 'drizzle-orm/node-postgres';"));
        assert!(source_imports_db_library("import { PrismaClient } from \"@prisma/client\";"));
        assert!(source_imports_db_library("const pg = require('pg');"));
        assert!(source_imports_db_library("const m = await import(\"mongodb\");"));
    }

    #[test]
    fn source_imports_db_library_rejects_non_db_specifiers() {
        // PGlite (electric-sql) is an in-browser WASM Postgres not in DB_PACKAGES,
        // so a REPL-demo component importing it is not gated as a DB-access file.
        assert!(!source_imports_db_library("import { PGlite } from '@electric-sql/pglite';"));
        assert!(!source_imports_db_library("import { ref } from 'vue';"));
        // A bare `pg` outside quotes (an identifier or prose) must not match.
        assert!(!source_imports_db_library("const pgClient = makePg();"));
    }

    fn is_solid(src: &str) -> bool {
        let project = crate::project::ProjectCtx::for_test_with_files(&[]);
        super::is_solid_file(src, &project, std::path::Path::new("t.tsx"))
    }

    #[test]
    fn is_solid_file_detects_import_context() {
        assert!(is_solid("import { render } from \"solid-js\";"));
        assert!(is_solid("import { render } from 'solid-js';"));
        assert!(is_solid("import { render } from \"solid-js/web\";"));
        assert!(is_solid("import { createStore } from \"solid-js/store\";"));
        assert!(is_solid("import { Router } from \"@solidjs/router\";"));
        assert!(is_solid("import { StartServer } from \"@solidjs/start\";"));
        assert!(is_solid("import { createRouteAction } from \"solid-start\";"));
        assert!(is_solid("import { redirect } from \"solid-start/server\";"));
        assert!(is_solid("import { Link } from \"@tanstack/solid-router\";"));
        assert!(is_solid("const s = require('solid-js');"));
        assert!(is_solid("/** @jsxImportSource solid-js */\nlet C = () => <div/>;"));
    }

    #[test]
    fn is_solid_file_rejects_non_import_mentions() {
        // Issue #7075: a Solid package name that appears only inside a URL path
        // string, comment, or data literal is not an import and must not mark a
        // non-Solid (React/Next) file as Solid.
        assert!(!is_solid("const links = [{ href: \"/docs/integrations/solid-start\" }];"));
        assert!(!is_solid("// see the solid-js docs for @solidjs/router details"));
        assert!(!is_solid("const label = \"Built with solid-js\";"));
        assert!(!is_solid("import { useState } from \"react\";"));
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

    fn use_imported_from_react(src: &str) -> bool {
        super::with_oxc_parse(src, std::path::Path::new("t.tsx"), |semantic| {
            super::is_imported_from_react("use", semantic)
        })
    }

    #[test]
    fn imported_from_react_matches_react_and_react_dom() {
        assert!(use_imported_from_react("import { use } from 'react';"));
        assert!(use_imported_from_react("import { use } from 'react-dom';"));
    }

    #[test]
    fn imported_from_react_rejects_other_sources_and_local() {
        assert!(!use_imported_from_react("import { use } from '../../hooks';"));
        assert!(!use_imported_from_react("import { use } from 'preact';"));
        assert!(!use_imported_from_react("function use(p) {}"));
        assert!(!use_imported_from_react("const x = 1;"));
    }
}
