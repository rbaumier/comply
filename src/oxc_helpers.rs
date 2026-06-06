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

use rustc_hash::FxHashMap;

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
const FILE_BOOL_SLOTS: usize = 4;

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

#[cfg(test)]
mod oxc_helpers_tests {
    use super::{byte_offset_to_line_col, reset_file_caches, source_contains};

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
}
