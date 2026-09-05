//! rust-hashmap-integer-key backend.
//!
//! Walks `generic_type` nodes whose base is written exactly `HashMap`/`HashSet`
//! or `std::collections::HashMap`/`HashSet`, in a *declaration* position — a
//! struct field, a `let` annotation, a parameter, a return type, a type alias,
//! a `static`/`const`. An expression turbofish (`HashMap::<u64, V>::new()`) and
//! any other usage position is left alone: the declaration is where the type is
//! chosen, and reporting both would double-count one decision.
//!
//! The key — the first type argument — must be an integer: a primitive
//! (`u8`…`u128`, `i8`…`i128`, `usize`, `isize`), a tuple of them, or a newtype
//! the same file declares over one (`struct BlockId(u32);`).
//!
//! Not flagged, each because the premise fails rather than because the case is
//! rare:
//!
//! - a third type argument (`HashMap<u64, V, FxBuildHasher>`) — the hasher is
//!   already chosen, SipHash is not in play;
//! - `FxHashMap`, `AHashMap`, `IndexMap`, `DashMap` and every other named
//!   container: only `std`'s two carry the SipHash default;
//! - a file that aliases the name (`use rustc_hash::FxHashMap as HashMap;`),
//!   where `HashMap<u64, V>` already means the fast one. The exemption applies
//!   only to the bare spelling — a fully qualified
//!   `std::collections::HashMap<u64, V>` names `std`'s type whatever the file
//!   imports;
//! - a non-integer key (`String`, `&str`, an unresolved generic parameter);
//! - test code and benchmarks, where the hasher choice is not the code under
//!   study.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{file_binds_name_to_module, is_test_code, root_node};

const INTEGER_PRIMITIVES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// Node kinds that only wrap a type inside a larger type. The walk from a
/// `generic_type` to whatever declares it passes through these transparently,
/// so `Arc<HashMap<u64, V>>` and `&HashMap<u64, V>` reach the same owners as a
/// bare `HashMap<u64, V>`.
const TYPE_WRAPPER_KINDS: &[&str] = &[
    "generic_type",
    "type_arguments",
    "type_binding",
    "reference_type",
    "pointer_type",
    "tuple_type",
    "array_type",
    "parenthesized_type",
];

/// Nodes that *declare* the type they contain. Reaching one of these means the
/// `HashMap` spelling here is the one a fix has to edit.
const DECLARATION_OWNERS: &[&str] = &[
    "field_declaration",
    "ordered_field_declaration_list",
    "parameter",
    "let_declaration",
    "type_item",
    "static_item",
    "const_item",
    // Reached through the `return_type` field only — a `HashMap` in the body
    // goes through `block`, which is in neither list.
    "function_item",
    "function_signature_item",
];

crate::ast_check! { on ["generic_type"] prefilter = ["HashMap", "HashSet"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir || ctx.file.path_segments.in_benchmark_dir { return; }
    if is_test_code(node, source, ctx) { return; }
    if !is_in_declaration_position(node) { return; }

    let Some(type_node) = node.child_by_field_name("type") else { return; };
    let Ok(written) = type_node.utf8_text(source) else { return; };
    let Some(container) = std_hash_container(written) else { return; };

    let Some(type_arguments) = node.child_by_field_name("type_arguments") else { return; };
    let mut cursor = type_arguments.walk();
    let written_arguments: Vec<tree_sitter::Node> = type_arguments
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "lifetime")
        .collect();
    // A hasher argument past the key (and value) means the author already picked
    // the hasher; there is nothing left to report.
    let expected_arity = if container == "HashMap" { 2 } else { 1 };
    if written_arguments.len() > expected_arity { return; }

    let Some(key) = written_arguments.first() else { return; };
    if !key_is_integer(*key, source) { return; }

    // `use rustc_hash::FxHashMap as HashMap;` makes the bare name mean the fast
    // map already. A qualified spelling cannot be aliased away, so it stays.
    if written == container && file_aliases_container(node, source, container) { return; }

    let key_text = key.utf8_text(source).unwrap_or("_");
    // A `HashSet` takes the key alone; only a `HashMap` has a value to elide.
    let value_slot = if container == "HashMap" { ", …" } else { "" };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{container}<{key_text}{value_slot}>` hashes an integer key with SipHash, whose DoS resistance buys nothing here \
             and costs more than the lookup. Use `rustc_hash::FxHashMap` (or `ahash::AHashMap`) and switch the \
             constructor to `FxHashMap::default()` / `with_capacity_and_hasher`."
        ),
        Severity::Error,
    ));
}

/// The container a base-type spelling names, or `None` for anything that is not
/// one of `std`'s two SipHash-defaulted maps.
fn std_hash_container(written: &str) -> Option<&'static str> {
    match written {
        "HashMap" | "collections::HashMap" | "std::collections::HashMap" => Some("HashMap"),
        "HashSet" | "collections::HashSet" | "std::collections::HashSet" => Some("HashSet"),
        _ => None,
    }
}

/// True when the type sits in a position that declares it, rather than merely
/// mentioning it. Climbs past the wrapper types (`Arc<…>`, `&…`, `Vec<…>`) and
/// answers on the first ancestor that is neither.
fn is_in_declaration_position(node: tree_sitter::Node) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if !TYPE_WRAPPER_KINDS.contains(&parent.kind()) {
            return DECLARATION_OWNERS.contains(&parent.kind());
        }
        current = parent;
    }
    false
}

/// True when a key type is an integer: a primitive, a tuple whose every element
/// is one, or a newtype this file declares over one.
fn key_is_integer(key: tree_sitter::Node, source: &[u8]) -> bool {
    if is_integer_shape(key, source) {
        return true;
    }
    key.kind() == "type_identifier"
        && key
            .utf8_text(source)
            .is_ok_and(|name| file_declares_integer_newtype(key, name, source))
}

/// True for an integer primitive or a tuple whose every element is one.
/// Deliberately blind to newtypes: [`file_declares_integer_newtype`] resolves
/// exactly one hop through this function, so a chain (`struct A(B);`) is not
/// followed and a self-referential `struct A(A);` cannot loop the walk.
fn is_integer_shape(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "primitive_type" => node
            .utf8_text(source)
            .is_ok_and(|text| INTEGER_PRIMITIVES.contains(&text)),
        "tuple_type" => {
            let mut cursor = node.walk();
            let mut elements = node.named_children(&mut cursor).peekable();
            elements.peek().is_some() && elements.all(|element| is_integer_shape(element, source))
        }
        _ => false,
    }
}

/// True when the file declares `struct <name>(<integer>);` — a newtype whose
/// hashing cost is the wrapped integer's. A generic parameter `K` resolves to
/// nothing here and answers false, which is what keeps
/// `fn f<K>(m: HashMap<K, V>)` unflagged.
fn file_declares_integer_newtype(node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let root = root_node(node);
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if current.kind() == "struct_item"
            && current
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                == Some(name)
            && let Some(field) = single_tuple_field(current)
            && is_integer_shape(field, source)
        {
            return true;
        }
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// The single wrapped type of a tuple struct, or `None` for every other struct
/// shape.
fn single_tuple_field(struct_item: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let body = struct_item.child_by_field_name("body")?;
    if body.kind() != "ordered_field_declaration_list" {
        return None;
    }
    let mut cursor = body.walk();
    let mut types = body.children_by_field_name("type", &mut cursor);
    let first = types.next()?;
    types.next().is_none().then_some(first)
}

/// True when the file binds the bare container name to something other than
/// `std::collections` — the `use rustc_hash::FxHashMap as HashMap;` idiom, and
/// equally `use hashbrown::HashMap;`. Both already carry a fast hasher.
fn file_aliases_container(node: tree_sitter::Node, source: &[u8], container: &str) -> bool {
    file_binds_name_to_module(node, source, container, |module| {
        !(module.len() == 2 && module[0] == "std" && module[1] == "collections")
    })
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_u64_keyed_field() {
        assert_eq!(run("struct S { by_id: HashMap<u64, String> }").len(), 1);
    }

    #[test]
    fn flags_usize_keyed_let_binding() {
        let src = "fn f() { let seen: HashSet<usize> = HashSet::new(); let _ = seen; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_qualified_std_hash_map_parameter() {
        assert_eq!(run("fn f(m: &std::collections::HashMap<u32, u8>) {}").len(), 1);
    }

    #[test]
    fn flags_tuple_of_integers_key_in_return_type() {
        assert_eq!(run("fn f() -> HashMap<(u16, u16), Cell> { todo!() }").len(), 1);
    }

    #[test]
    fn flags_newtype_key_declared_in_the_file() {
        let src = "struct BlockId(u32);\nstruct S { blocks: HashMap<BlockId, Block> }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_nested_hash_map_once_per_occurrence() {
        assert_eq!(run("struct S { m: HashMap<u8, HashMap<u16, V>> }").len(), 2);
    }

    #[test]
    fn flags_hash_map_behind_a_wrapper_type() {
        assert_eq!(run("struct S { m: Arc<HashMap<u64, V>> }").len(), 1);
    }

    #[test]
    fn flags_type_alias() {
        assert_eq!(run("type Cache = HashMap<u64, Vec<u8>>;").len(), 1);
    }

    #[test]
    fn allows_string_key() {
        assert!(run("struct S { by_name: HashMap<String, u64> }").is_empty());
    }

    #[test]
    fn allows_str_key() {
        assert!(run("struct S<'a> { by_name: HashMap<&'a str, u64> }").is_empty());
    }

    #[test]
    fn allows_explicit_hasher_argument() {
        assert!(run("struct S { m: HashMap<u64, V, FxBuildHasher> }").is_empty());
    }

    #[test]
    fn allows_fx_hash_map() {
        assert!(run("struct S { m: FxHashMap<u64, V> }").is_empty());
    }

    #[test]
    fn allows_index_map_and_dash_map() {
        assert!(run("struct S { a: IndexMap<u64, V>, b: DashMap<u32, V> }").is_empty());
    }

    #[test]
    fn allows_file_that_aliases_hash_map_to_a_fast_one() {
        let src = "use rustc_hash::FxHashMap as HashMap;\nstruct S { m: HashMap<u64, V> }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_qualified_std_spelling_even_when_the_file_aliases_the_bare_name() {
        let src = "use rustc_hash::FxHashMap as HashMap;\nstruct S { m: std::collections::HashMap<u64, V> }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_unresolved_generic_key() {
        assert!(run("fn f<K, V>(m: HashMap<K, V>) {}").is_empty());
    }

    #[test]
    fn allows_newtype_over_a_string() {
        let src = "struct Name(String);\nstruct S { m: HashMap<Name, V> }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_expression_turbofish() {
        let src = "fn f() { let m = HashMap::<u64, String>::new(); let _ = m; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        let src = "#[cfg(test)]\nmod tests { struct S { m: HashMap<u64, V> } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_benchmark_file() {
        let file = FileCtx {
            path_segments: PathSegments { in_benchmark_dir: true, ..Default::default() },
            ..Default::default()
        };
        let diagnostics = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            "struct S { m: HashMap<u64, V> }",
            "benches/bench.rs",
            crate::project::default_static_project_ctx(),
            &file,
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn allows_hash_map_beside_a_plain_std_import() {
        let src = "use std::collections::HashMap;\nstruct S { m: HashMap<String, V> }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_hash_map_with_a_plain_std_import() {
        let src = "use std::collections::HashMap;\nstruct S { m: HashMap<u64, V> }";
        assert_eq!(run(src).len(), 1);
    }
}
