//! rust-prefer-fast-hasher backend.
//!
//! Walks `generic_type` nodes. Matches the base type name (trailing
//! segment) against `HashMap` / `HashSet`. Inspects `type_arguments`:
//!
//! - `HashMap<K, V>` with exactly two type args where `K` is a
//!   primitive integer → flag.
//! - `HashSet<K>` with exactly one type arg where `K` is a primitive
//!   integer → flag.
//!
//! Any explicit hasher (e.g. `HashMap<K, V, FxBuildHasher>`) pushes the
//! arg count past the threshold and is left alone — the user has
//! already opted into a custom hasher.

use crate::diagnostic::{Diagnostic, Severity};

const INT_PRIMITIVES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

crate::ast_check! { on ["generic_type"] => |node, source, ctx, diagnostics|
    let Some(type_node) = node.child_by_field_name("type") else { return; };
    let type_text = type_node.utf8_text(source).unwrap_or("");
    // Accept `HashMap`, `std::collections::HashMap`, etc. — trailing segment wins.
    let base = type_text.rsplit("::").next().unwrap_or("");
    let (expected_args, map_label): (usize, &str) = match base {
        "HashMap" => (2, "HashMap"),
        "HashSet" => (1, "HashSet"),
        _ => return,
    };

    let Some(args) = node.child_by_field_name("type_arguments") else { return; };

    // Count named children — each named child is one type argument.
    let mut cursor = args.walk();
    let named: Vec<tree_sitter::Node> = args.named_children(&mut cursor).collect();
    if named.len() != expected_args { return; }

    let key_node = named[0];
    if key_node.kind() != "primitive_type" { return; }
    let key_text = key_node.utf8_text(source).unwrap_or("");
    if !INT_PRIMITIVES.contains(&key_text) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "{map_label} with integer key `{key_text}` uses SipHash by default — prefer ahash/FxHashMap for better perf."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_hashmap_u64_key() {
        assert_eq!(
            run("fn f() -> HashMap<u64, String> { todo!() }").len(),
            1
        );
    }

    #[test]
    fn flags_hashset_usize() {
        assert_eq!(run("fn f() -> HashSet<usize> { todo!() }").len(), 1);
    }

    #[test]
    fn allows_hashmap_string_key() {
        assert!(run("fn f() -> HashMap<String, u64> { todo!() }").is_empty());
    }

    #[test]
    fn allows_hashmap_with_explicit_hasher() {
        assert!(
            run("fn f() -> HashMap<u64, String, FxBuildHasher> { todo!() }").is_empty()
        );
    }

    #[test]
    fn allows_ahashmap() {
        assert!(run("fn f() -> AHashMap<u64, String> { todo!() }").is_empty());
    }
}
