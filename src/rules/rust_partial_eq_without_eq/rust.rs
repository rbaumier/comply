//! rust-partial-eq-without-eq backend.
//!
//! Walks every `struct_item` / `enum_item` and reads its outer
//! attributes plus any sibling `impl PartialEq for T` / `impl Eq
//! for T` blocks in the same file. If `PartialEq` is *derived* but
//! `Eq` is missing, we emit a diagnostic at the type definition.
//!
//! Types in a test context (`#[cfg(test)]` module, `#[test]` fn) are
//! skipped: they are throwaway fixtures deriving `PartialEq` only for
//! `assert_eq!`, so a missing `Eq` is not a defect.
//!
//! A *manual* `impl PartialEq` is out of scope: it is the author's
//! explicit opt-out from standard reflexive equality (a hand-written
//! `eq` may be non-reflexive), so we never demand `Eq` for it.
//!
//! A type is flagged only when *every* field type is **provably `Eq`**
//! — the conservative gate that keeps "add `Eq`" compilable. A field
//! type is provably `Eq` when, walking its AST:
//!
//! * it is a non-float primitive, the unit type, or a known-`Eq`
//!   stdlib leaf (`String`, `str`, `PathBuf`, `Path`, `OsString`,
//!   `OsStr`, `bool`, `char`, the integer primitives);
//! * it is a known-`Eq`-when-args-`Eq` stdlib container (`Option`,
//!   `Vec`, `Box`, `Rc`, `Arc`, `VecDeque`, `BTreeMap`, `BTreeSet`,
//!   `HashMap`, `HashSet`, `Cow`) whose every type argument is itself
//!   provably `Eq`;
//! * it is a reference (`&T`), array (`[T; N]`) or tuple `(A, B)`
//!   whose every contained type is provably `Eq`;
//! * it names a locally-defined type not known to force partial-only
//!   equality (resolved via the per-file Eq-incapable memo).
//!
//! Anything else — an `f32` / `f64`, an imported or otherwise unknown
//! bare/path-qualified type, a generic type parameter (`T`), a `dyn` /
//! `impl` / fn-pointer type — is **not** provably `Eq`, so the rule
//! stays silent: it cannot prove `#[derive(Eq)]` would compile. This
//! covers cross-file float-bearing field types (`use crate::Rewrites`)
//! and generic types (`Source<'a, T>`) that the current-file-only
//! resolution can never see.

use std::collections::HashSet;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

const KINDS: &[&str] = &["struct_item", "enum_item"];

/// Per-file memo of type names defined in the file that cannot
/// implement `Eq` — either because a field is (transitively)
/// float-bearing, or because the type is itself `PartialEq` without
/// `Eq`. Computed once on the first visit.
type EqIncapableTypeNames = Option<HashSet<String>>;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        let memo: EqIncapableTypeNames = None;
        Some(Box::new(memo))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        // A `#[cfg(test)]`-gated type is a throwaway fixture that derives
        // `PartialEq` only for `assert_eq!`; it never ships, so its lack of
        // `Eq` (no `HashSet` / `BTreeSet` use) is not a defect.
        if is_in_test_context(node, source_bytes) {
            return;
        }
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(type_name) = name_node.utf8_text(source_bytes) else {
            return;
        };
        let eq_incapable_names = state.and_then(|s| s.downcast_mut::<EqIncapableTypeNames>());
        // Flag only when every field type is provably `Eq`. A field whose
        // `Eq`-ness the rule cannot prove — a float, an imported/unknown
        // type, or a generic type parameter — leaves the type exempt, so
        // we never suggest a non-compilable `#[derive(Eq)]`.
        if !all_fields_provably_eq(node, source_bytes, eq_incapable_names) {
            return;
        }
        let derives = collect_derives(node, source_bytes);
        let traits = search_traits_in_root(node, source_bytes, type_name, &derives);
        // A hand-written `impl PartialEq` is the author's explicit opt-out
        // from standard reflexive equality; "add `Eq`" is not automatable.
        if traits.partial_eq_is_manual {
            return;
        }
        if traits.has_partial_eq && !traits.has_eq {
            diagnostics.push(Diagnostic::at_node(
                std::sync::Arc::clone(&ctx.path_arc),
                &name_node,
                "rust-partial-eq-without-eq",
                format!(
                    "`{type_name}` implements `PartialEq` but not `Eq`. \
                     Add `Eq` (derive or manual impl) — `Eq` documents that \
                     equality is reflexive and unlocks `HashSet` / `BTreeSet` \
                     usage."
                ),
                Severity::Warning,
            ));
        }
    }
}

/// Returns `true` only when *every* field type of the struct/enum is
/// provably `Eq`, so adding `Eq` is guaranteed to compile. A field whose
/// `Eq`-ness cannot be proven — a float, an imported/unknown type name, a
/// generic type parameter — makes this `false`, exempting the type.
///
/// `eq_incapable_names` memoizes the set of locally-defined type names
/// known to force partial-only equality so it is computed once per file
/// rather than per visited type.
fn all_fields_provably_eq(
    node: tree_sitter::Node,
    source: &[u8],
    eq_incapable_names: Option<&mut EqIncapableTypeNames>,
) -> bool {
    match eq_incapable_names {
        Some(memo) => {
            let names = memo.get_or_insert_with(|| collect_eq_incapable_type_names(node, source));
            type_def_all_fields_provably_eq(node, source, names)
        }
        // No state available (defensive): no local Eq-incapable names known.
        None => type_def_all_fields_provably_eq(node, source, &HashSet::new()),
    }
}

/// Whether every field type of the struct/enum is provably `Eq` given the
/// set of known Eq-incapable local type names. An empty struct/enum (no
/// fields) is vacuously all-provably-`Eq`, matching `#[derive(Eq)]` on a
/// field-less type.
fn type_def_all_fields_provably_eq(
    node: tree_sitter::Node,
    source: &[u8],
    eq_incapable_names: &HashSet<String>,
) -> bool {
    field_type_nodes(node)
        .iter()
        .all(|ty| field_type_is_provably_eq(*ty, source, eq_incapable_names))
}

/// Leaf type names that are unconditionally `Eq` in std.
const EQ_STDLIB_LEAVES: &[&str] = &[
    "String", "str", "OsString", "OsStr", "PathBuf", "Path", "bool", "char", "u8", "u16", "u32",
    "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// Container type names that are `Eq` iff all their type arguments are
/// `Eq`. (`Cow<'a, T>` is `Eq` iff `T::Owned: Eq`, which holds for every
/// std type these contain.)
const EQ_STDLIB_CONTAINERS: &[&str] = &[
    "Option", "Vec", "Box", "Rc", "Arc", "VecDeque", "BTreeMap", "BTreeSet", "HashMap", "HashSet",
    "Cow",
];

/// Whether a field-type AST node is *provably* `Eq`, i.e. the rule can
/// statically prove `#[derive(Eq)]` would compile for a field of this
/// type. Conservative: any type it cannot prove `Eq` (imported/unknown
/// names, generic params, `dyn` / `impl` / fn-pointers) yields `false`.
fn field_type_is_provably_eq(
    ty: tree_sitter::Node,
    source: &[u8],
    eq_incapable_names: &HashSet<String>,
) -> bool {
    match ty.kind() {
        // A non-float primitive is `Eq`; `f32` / `f64` never are.
        "primitive_type" => ty
            .utf8_text(source)
            .is_ok_and(|text| text != "f32" && text != "f64"),
        // The unit type `()` is `Eq`.
        "unit_type" => true,
        // `&T` / `&mut T` and `[T; N]`: `Eq` iff the referenced/element
        // type is `Eq`. The length expression of an array is not a type.
        "reference_type" | "array_type" => ty
            .child_by_field_name(if ty.kind() == "array_type" {
                "element"
            } else {
                "type"
            })
            .is_some_and(|inner| field_type_is_provably_eq(inner, source, eq_incapable_names)),
        // `(A, B)`: `Eq` iff every element type is `Eq`. Elements are the
        // named `_type` children (no field names on a tuple).
        "tuple_type" => named_type_children(ty)
            .all(|elem| field_type_is_provably_eq(elem, source, eq_incapable_names)),
        // A bare type name: a known-`Eq` stdlib leaf, or a local type not
        // in the Eq-incapable memo. Anything else (imported/unknown,
        // generic param) is not provably `Eq`.
        "type_identifier" => {
            let Ok(text) = ty.utf8_text(source) else {
                return false;
            };
            EQ_STDLIB_LEAVES.contains(&text)
                || (!eq_incapable_names.contains(text) && is_local_type_name(ty, source, text))
        }
        // A possibly-generic type, e.g. `Option<u32>`, `Vec<T>`,
        // `crate::Rewrites`. Only the known-`Eq` stdlib containers are
        // provably `Eq`, and only when every type argument is `Eq`.
        "generic_type" | "scoped_type_identifier" => {
            generic_container_is_provably_eq(ty, source, eq_incapable_names)
        }
        // Unknown/unhandled (`dyn`, `impl`, fn-pointer, raw pointer,
        // lifetime-only, ...): not provably `Eq`.
        _ => false,
    }
}

/// Named type children of a tuple type node, skipping lifetimes.
fn named_type_children(node: tree_sitter::Node) -> impl Iterator<Item = tree_sitter::Node> {
    let mut cursor = node.walk();
    let children: Vec<_> = node
        .children(&mut cursor)
        .filter(|c| c.is_named() && c.kind() != "lifetime")
        .collect();
    children.into_iter()
}

/// Whether a generic / path-qualified type is provably `Eq`. A
/// `scoped_type_identifier` (`crate::Foo`, `std::path::Path`) is treated
/// by its final segment only when it is a known stdlib leaf; otherwise it
/// is an imported/unknown type and not provably `Eq`. A `generic_type`
/// (`Option<u32>`, `HashMap<K, V>`) is provably `Eq` iff its base is a
/// known-`Eq` stdlib container and every type argument is provably `Eq`.
fn generic_container_is_provably_eq(
    ty: tree_sitter::Node,
    source: &[u8],
    eq_incapable_names: &HashSet<String>,
) -> bool {
    if ty.kind() == "scoped_type_identifier" {
        // Path-qualified bare type, e.g. `std::path::Path`. Only a known
        // stdlib leaf is provably `Eq`; any other path is foreign.
        return ty
            .utf8_text(source)
            .ok()
            .and_then(|t| t.rsplit("::").next())
            .is_some_and(|name| EQ_STDLIB_LEAVES.contains(&name));
    }
    // `generic_type`: resolve the base type name and the argument list.
    let Some(base) = ty.child_by_field_name("type") else {
        return false;
    };
    let base_name = match base.kind() {
        "type_identifier" => base.utf8_text(source).ok(),
        // `std::collections::HashMap<..>`: take the final path segment.
        "scoped_type_identifier" => base.utf8_text(source).ok().and_then(|t| t.rsplit("::").next()),
        _ => None,
    };
    let Some(base_name) = base_name else {
        return false;
    };
    if !EQ_STDLIB_CONTAINERS.contains(&base_name) {
        return false;
    }
    let Some(args) = ty.child_by_field_name("type_arguments") else {
        // A container written without args is not something we expect;
        // be conservative.
        return false;
    };
    // Every type argument must itself be provably `Eq`. Non-type
    // arguments (lifetimes, const generics) are skipped.
    let mut cursor = args.walk();
    args.children(&mut cursor)
        .filter(|c| c.is_named() && c.kind() != "lifetime")
        .all(|arg| field_type_is_provably_eq(arg, source, eq_incapable_names))
}

/// Whether `name` is a type defined in the current file (so the rule can
/// reason about its `Eq`-ability via the memo). An unknown name is
/// assumed imported / external and thus not provably `Eq`.
fn is_local_type_name(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut root = node;
    while let Some(p) = root.parent() {
        root = p;
    }
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if KINDS.contains(&n.kind())
            && let Some(name_node) = n.child_by_field_name("name")
            && name_node.utf8_text(source).is_ok_and(|t| t == name)
        {
            return true;
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// Whether any field type of the struct/enum forces partial-only equality
/// given the set of known Eq-incapable local type names.
fn type_def_has_eq_incapable_field(
    node: tree_sitter::Node,
    source: &[u8],
    eq_incapable_names: &HashSet<String>,
) -> bool {
    field_type_nodes(node)
        .iter()
        .any(|ty| type_node_forces_partial_eq(*ty, source, eq_incapable_names))
}

/// Collects the field-type nodes of a struct/enum definition. Covers
/// named fields (`field_declaration`), tuple fields
/// (`ordered_field_declaration_list`) and enum variant payloads.
fn field_type_nodes(node: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            match child.kind() {
                "field_declaration" => {
                    if let Some(ty) = child.child_by_field_name("type") {
                        out.push(ty);
                    }
                }
                // Tuple-field type: `struct A(f64, Inner)`. The type nodes
                // sit directly inside `ordered_field_declaration_list`,
                // interleaved with visibility/attribute nodes.
                "ordered_field_declaration_list" => {
                    let mut tc = child.walk();
                    for grand in child.children(&mut tc) {
                        if grand.is_named()
                            && grand.kind() != "attribute_item"
                            && grand.kind() != "visibility_modifier"
                        {
                            out.push(grand);
                        }
                    }
                }
                // Descend into containers that hold variants/fields.
                "field_declaration_list" | "enum_variant_list" | "enum_variant" => {
                    stack.push(child);
                }
                _ => {}
            }
        }
    }
    out
}

/// Whether a field-type AST node forces partial-only equality: it
/// contains an `f32` / `f64` `primitive_type` anywhere (arrays, tuples,
/// references, generic arguments) or names a known Eq-incapable local
/// type.
fn type_node_forces_partial_eq(
    ty: tree_sitter::Node,
    source: &[u8],
    eq_incapable_names: &HashSet<String>,
) -> bool {
    let mut stack = vec![ty];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "primitive_type" => {
                if let Ok(text) = n.utf8_text(source)
                    && (text == "f32" || text == "f64")
                {
                    return true;
                }
            }
            "type_identifier" => {
                if let Ok(text) = n.utf8_text(source)
                    && eq_incapable_names.contains(text)
                {
                    return true;
                }
            }
            _ => {}
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// Computes, via fixpoint, the set of type names defined in the file
/// that cannot implement `Eq`, so a field of such a type makes
/// `#[derive(Eq)]` uncompilable. A type is Eq-incapable when it is
/// itself `PartialEq` without `Eq` (a manual `impl PartialEq`, or a
/// derived `PartialEq` with no `Eq`), or when any field is a direct
/// `f32` / `f64` or names another type already known to be Eq-incapable.
fn collect_eq_incapable_type_names(node: tree_sitter::Node, source: &[u8]) -> HashSet<String> {
    let mut root = node;
    while let Some(p) = root.parent() {
        root = p;
    }

    // Gather every local type definition with its name node.
    let mut defs: Vec<(String, tree_sitter::Node)> = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if KINDS.contains(&n.kind())
            && let Some(name_node) = n.child_by_field_name("name")
            && let Ok(name) = name_node.utf8_text(source)
        {
            defs.push((name.to_string(), n));
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }

    let mut eq_incapable_names = HashSet::new();
    // Seed: any local type that is `PartialEq` without `Eq` cannot gain
    // `Eq`, so it taints any type that holds it as a field.
    for (name, def) in &defs {
        let derives = collect_derives(*def, source);
        let traits = search_traits_in_root(*def, source, name, &derives);
        if traits.has_partial_eq && !traits.has_eq {
            eq_incapable_names.insert(name.clone());
        }
    }
    loop {
        let mut changed = false;
        for (name, def) in &defs {
            if eq_incapable_names.contains(name) {
                continue;
            }
            if type_def_has_eq_incapable_field(*def, source, &eq_incapable_names) {
                eq_incapable_names.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    eq_incapable_names
}

fn collect_derives(node: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    // Outer attributes are siblings *before* the type, attached as
    // `attribute_item` children of the parent declaration list.
    let Some(parent) = node.parent() else {
        return out;
    };
    let mut cursor = parent.walk();
    let children: Vec<_> = parent.children(&mut cursor).collect();
    let Some(idx) = children.iter().position(|c| c.id() == node.id()) else {
        return out;
    };
    // Walk backward through preceding siblings while they're attribute_items.
    for i in (0..idx).rev() {
        let c = children[i];
        if c.kind() != "attribute_item" {
            break;
        }
        let Ok(text) = c.utf8_text(source) else {
            continue;
        };
        if let Some(start) = text.find("derive(") {
            let after = &text[start + "derive(".len()..];
            if let Some(end) = after.find(')') {
                let list = &after[..end];
                for item in list.split(',') {
                    out.push(item.trim().to_string());
                }
            }
        }
    }
    out
}

/// How `PartialEq` / `Eq` are provided for a type, combining derives
/// with any `impl Trait for TypeName` blocks at the file root.
struct TraitInfo {
    has_partial_eq: bool,
    has_eq: bool,
    /// `PartialEq` is provided by a hand-written `impl` block (not a
    /// `#[derive]`). Such equality may be non-reflexive, so `Eq` is not
    /// safely addable.
    partial_eq_is_manual: bool,
}

fn search_traits_in_root(
    node: tree_sitter::Node,
    source: &[u8],
    type_name: &str,
    derives: &[String],
) -> TraitInfo {
    let mut info = TraitInfo {
        has_partial_eq: derives.iter().any(|d| d == "PartialEq"),
        has_eq: derives.iter().any(|d| d == "Eq"),
        partial_eq_is_manual: false,
    };
    // Walk the entire tree for `impl_item` blocks targeting this type.
    let mut root = node;
    while let Some(p) = root.parent() {
        root = p;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "impl_item" {
            let trait_node = n.child_by_field_name("trait");
            let target_node = n.child_by_field_name("type");
            if let (Some(tr), Some(tg)) = (trait_node, target_node) {
                let trait_text = tr.utf8_text(source).unwrap_or("");
                let target_text = tg.utf8_text(source).unwrap_or("");
                if target_text == type_name {
                    let bare = trait_text.rsplit("::").next().unwrap_or(trait_text);
                    if bare == "PartialEq" {
                        info.has_partial_eq = true;
                        info.partial_eq_is_manual = true;
                    } else if bare == "Eq" {
                        info.has_eq = true;
                    }
                }
            }
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    info
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
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_struct_with_partial_eq_only() {
        let source = "#[derive(PartialEq)]\nstruct A { x: i32 }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_struct_in_cfg_test_module() {
        // Issue #3839: a `#[cfg(test)]`-gated fixture derives `PartialEq` only
        // so `assert_eq!` works; it never ships, so demanding `Eq` is ceremony.
        let source = "\
#[cfg(test)]
mod tests {
    #[derive(PartialEq, Debug)]
    struct Fixture { x: i32 }
}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_struct_with_both() {
        let source = "#[derive(PartialEq, Eq)]\nstruct A { x: i32 }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_struct_with_float_field() {
        let source = "#[derive(PartialEq)]\nstruct A { x: f64 }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_enum_with_partial_eq_only() {
        let source = "#[derive(PartialEq)]\nenum E { A, B }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_struct_with_no_eq_at_all() {
        let source = "struct A { x: i32 }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_tuple_struct_with_float() {
        let source = "#[derive(PartialEq)]\nstruct A(f64);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_struct_with_float_array_field() {
        let source = "#[derive(PartialEq)]\nstruct A { v: [f32; 3] }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_struct_with_float_tuple_field() {
        let source = "#[derive(PartialEq)]\nstruct A { p: (f64, f64) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_struct_wrapping_local_float_newtype() {
        // Issue #1249: a type wrapping a locally-defined float newtype also
        // cannot implement `Eq`.
        let source = "\
struct Inner(f64);
#[derive(PartialEq)]
struct A(Inner);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_skip_on_identifier_containing_f64_substring() {
        // `config64` is not a float — the type must still be flagged.
        let source = "#[derive(PartialEq)]\nstruct A { config64: i32 }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn does_not_skip_on_f64_only_in_comment() {
        // A `f64` mention in a doc comment must not silence the rule.
        let source = "#[derive(PartialEq)]\nstruct A {\n    /// holds an f64-ish count\n    n: i32,\n}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_enum_with_manual_partial_eq_impl() {
        // Issue #3911 case 1: a hand-written, non-reflexive `impl PartialEq`
        // (diesel's `Error`) is the author's explicit opt-out — adding `Eq`
        // would lie about reflexivity.
        let source = "\
#[derive(Debug)]
enum Error {
    A(i32),
    NotFound,
    Other,
}
impl PartialEq for Error {
    fn eq(&self, other: &Error) -> bool {
        match (self, other) {
            (Error::A(a), Error::A(b)) => a == b,
            (&Error::NotFound, &Error::NotFound) => true,
            _ => false,
        }
    }
}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_struct_with_manual_partial_eq_impl() {
        let source = "\
struct S {
    x: i32,
}
impl PartialEq for S {
    fn eq(&self, other: &S) -> bool {
        self.x == other.x
    }
}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_enum_with_field_of_manual_partial_eq_type() {
        // Issue #3911 case 2: diesel's `ConnectionError` derives `PartialEq`
        // but holds an `Error` whose `Eq` is unimplementable, so `#[derive(Eq)]`
        // on `ConnectionError` would be a hard compile error (E0277).
        let source = "\
#[derive(Debug)]
enum Error {
    A(i32),
    NotFound,
}
impl PartialEq for Error {
    fn eq(&self, _other: &Error) -> bool {
        false
    }
}
#[derive(Debug, PartialEq)]
enum ConnectionError {
    BadConnection(String),
    CouldntSetupConfiguration(Error),
}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_struct_with_field_of_derived_partial_eq_only_type() {
        // A field whose local type derives `PartialEq` but not `Eq` also
        // makes `#[derive(Eq)]` uncompilable; the right fix is on `Inner`,
        // which the rule still flags directly.
        let source = "\
#[derive(PartialEq)]
struct Inner {
    x: i32,
}
#[derive(PartialEq)]
struct Outer {
    inner: Inner,
}";
        // Only `Inner` (the directly-fixable derived-PartialEq type) is flagged.
        let diags = run_on(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Inner`"));
    }

    #[test]
    fn allows_struct_with_cross_file_imported_field_type() {
        // Issue #3718: `Rewrites` is defined in another file (`use crate::Rewrites`)
        // and transitively holds an `Option<f32>`, so `Outcome` cannot be `Eq`.
        // The field type is unknown to the current file → not provably `Eq`.
        let source = "#[derive(PartialEq)]\nstruct Outcome { options: Rewrites, n: usize }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_generic_struct_with_cross_file_option_field() {
        // Issue #3718 sibling: `DiffLineStats` is cross-file and `T` is an
        // unbounded generic param — `#[derive(Eq)]` would add a `T: Eq` bound
        // the author omitted, and require `DiffLineStats: Eq` it cannot prove.
        let source =
            "#[derive(PartialEq)]\nstruct Source<'a, T> { diff: Option<DiffLineStats>, change: &'a T }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_struct_with_bare_generic_param_field() {
        // A field that is just a generic type parameter is never provably `Eq`.
        let source = "#[derive(PartialEq)]\nstruct W<T> { inner: T }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_struct_with_stdlib_eq_fields() {
        // `String`, `Vec<u32>` and `bool` are all provably `Eq`, so `Eq` is
        // safely addable and the type must be flagged.
        let source = "#[derive(PartialEq)]\nstruct A { name: String, ids: Vec<u32>, flag: bool }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_struct_with_local_eq_field() {
        // `Inner` is a local type not in the Eq-incapable memo (its only field
        // is `i32`), so it is provably `Eq` and `Outer` must be flagged.
        let source = "struct Inner { x: i32 }\n#[derive(PartialEq)]\nstruct Outer { i: Inner }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_struct_with_int_array_field() {
        // `[u8; 4]` is provably `Eq` (element `u8` is `Eq`; the length
        // expression is not a type), so the type must be flagged.
        let source = "#[derive(PartialEq)]\nstruct A { v: [u8; 4] }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_struct_with_eq_tuple_field() {
        // `(u32, bool)` is provably `Eq`, so the type must be flagged.
        let source = "#[derive(PartialEq)]\nstruct A { p: (u32, bool) }";
        assert_eq!(run_on(source).len(), 1);
    }
}
