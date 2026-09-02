//! rust-newtype-missing-derives backend.
//!
//! Walks every `struct_item` whose `body` is an `ordered_field_declaration_list`
//! holding exactly one `type` — the newtype shape `pub struct UserId(u64);` —
//! and reads the traits named by its preceding `#[derive(…)]` attributes. A
//! newtype missing `Clone`, `PartialEq` or `Eq` gets one diagnostic listing
//! every trait it should gain. `Debug` is out of scope — it is
//! `rust-impl-debug-on-public-types`' subject.
//!
//! The subject is a newtype over a *resolvable value type*: a primitive, a
//! known-`Eq` stdlib leaf, or a container of those ([`type_is_provably_eq`]).
//! That is the newtype idiom this rule is about — `UserId(u64)`, `Email(String)`
//! — and it is the only case where "add these derives" is guaranteed to compile.
//! A newtype over a type the check cannot resolve (`Override(Gitignore)`,
//! `hyper_request(Request<IncomingBody>)`) stays silent: the wrapped type may
//! implement neither `Clone` nor `Eq`, and a suggestion that does not build is
//! worse than no suggestion. Floats, trait objects, closures and resource handles
//! fall out of that same gate. `Hash` is listed only alongside an already-derived
//! `Eq` — `Hash` without `Eq` breaks the `a == b => hash(a) == hash(b)` contract
//! — and never triggers the diagnostic on its own.
//!
//! The struct must be visible outside its own module (`pub` or `pub(crate)`,
//! via [`is_pub_including_restricted`]): a module-private newtype is fixed in
//! place by whoever needs the trait, whereas a `pub` one can only be fixed here
//! (the orphan rule bars a downstream `impl Clone for upstream::Id`).
//!
//! Exempt shapes, each one a place where "add the derives" is wrong or would not
//! compile:
//!
//! - a generic newtype (`pub struct W<T>(T)`) — the derives would need `T` bounds
//!   the author may deliberately not want;
//! - a hand-written `impl Clone for X` / `impl PartialEq for X` in the same file
//!   — the author opted out of the derive on purpose;
//! - a `#[repr(transparent)]` wrapper over a raw pointer (`*const T` / `*mut T`),
//!   the FFI handle shape whose equality is identity, not value;
//! - a conditional `#[cfg_attr(…, derive(…))]`, whose trait list depends on a
//!   build configuration this check can't resolve;
//! - test code, where a fixture newtype derives only what its assertions need.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    any_outer_attribute, collect_top_level_derives, file_impls_trait_for_type, has_attribute_option,
    has_test_attribute, is_in_test_context, is_pub_including_restricted,
};

/// Types whose `Eq`-ness is known here: stdlib leaves that implement `Eq`, and
/// containers that implement it whenever their arguments do. A type text is
/// provably `Eq` when every path segment it names is in this set, so
/// `Vec<String>` and `Cow<'a, str>` qualify while `Gitignore` does not.
const PROVABLY_EQ_TYPES: &[&str] = &[
    // Leaves.
    "bool", "char", "str", "String", "Path", "PathBuf", "OsStr", "OsString", "u8", "u16", "u32",
    "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "Duration", "IpAddr",
    "Ipv4Addr", "Ipv6Addr", "SocketAddr", "TypeId", "Uuid", "NonZeroU8", "NonZeroU16",
    "NonZeroU32", "NonZeroU64", "NonZeroUsize",
    // Containers, `Eq` when their arguments are.
    "Vec", "VecDeque", "Box", "Option", "Result", "Rc", "Arc", "Cow", "BTreeMap", "BTreeSet",
    "HashMap", "HashSet", "Reverse",
];

/// Traits whose absence makes the newtype unusable to a caller. `Hash` is
/// deliberately absent: it is reported only as an extra when `Eq` is already
/// derived, never as a reason to fire.
const REQUIRED_DERIVES: &[&str] = &["Clone", "PartialEq", "Eq"];

crate::ast_check! { on ["struct_item"] prefilter = ["struct"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    // `is_in_test_context` reads the ANCESTORS' attributes, so a `#[cfg(test)]`
    // written on this very struct needs its own check.
    if is_in_test_context(node, source) || has_test_attribute(node, source) { return; }
    if !is_pub_including_restricted(node, source) { return; }
    // A generic newtype would need `T: Clone`-style bounds on every derive, a
    // constraint the author may have declined on purpose.
    if node.child_by_field_name("type_parameters").is_some() { return; }

    let Some(inner_type) = single_tuple_field_type(node) else { return; };
    let Ok(inner_text) = inner_type.utf8_text(source) else { return; };
    // Only a wrapped type whose traits are known here: over anything else the
    // derives this rule would ask for might not compile.
    if !type_is_provably_eq(inner_text) { return; }
    // A `#[repr(transparent)]` newtype over a raw pointer is an FFI handle: its
    // identity is the address, and `Clone`/`Eq` on it would claim a value
    // semantics the pointer does not have.
    if is_transparent_pointer_handle(node, source, inner_text) { return; }

    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let Ok(name) = name_node.utf8_text(source) else { return; };
    // A hand-written `impl Clone` / `impl PartialEq` is the author's explicit
    // opt-out from the derive — the wrapped value needs custom semantics.
    if file_impls_trait_for_type(node, source, &["Clone", "PartialEq"], name) { return; }
    // `#[cfg_attr(feature = "x", derive(Clone))]` derives under a build
    // configuration this check cannot resolve, so its trait list is unknown.
    if has_conditional_derive(node, source) { return; }

    let derives = collect_top_level_derives(node, source);
    let missing = missing_derives(&derives);
    if !missing.iter().any(|trait_name| REQUIRED_DERIVES.contains(trait_name)) { return; }

    let list = missing.join(", ");
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Newtype `{name}` is missing `{list}` — it inherits nothing from the `{inner_text}` it wraps, \
             so callers can't clone or compare it and no downstream crate can add the impls (orphan rule). \
             Add `#[derive({list})]`."
        ),
        Severity::Error,
    ));
}

/// The single wrapped type of a newtype: `struct_item`'s `body` must be an
/// `ordered_field_declaration_list` (the tuple-struct shape) holding exactly one
/// `type` field. A named-field struct, a unit struct, `S()` and a multi-field
/// tuple struct all yield `None` — none of them is a newtype.
fn single_tuple_field_type<'tree>(
    struct_item: tree_sitter::Node<'tree>,
) -> Option<tree_sitter::Node<'tree>> {
    let body = struct_item.child_by_field_name("body")?;
    if body.kind() != "ordered_field_declaration_list" {
        return None;
    }
    let mut cursor = body.walk();
    let mut types = body.children_by_field_name("type", &mut cursor);
    let first = types.next()?;
    types.next().is_none().then_some(first)
}

/// True when every type named in `type_text` is known to implement `Eq`, so
/// `#[derive(PartialEq, Eq)]` on the newtype is guaranteed to compile.
///
/// The text is cut into path segments on the type-composition punctuation
/// (`<>`, `,`, `&`, `*`, tuple and array brackets), which leaves one segment per
/// named type; each is reduced to its final `::` segment and looked up in
/// [`PROVABLY_EQ_TYPES`]. Lifetimes, array lengths and the pointer/reference
/// qualifiers carry no `Eq` obligation and are skipped. One unknown segment —
/// a local type, an imported one, a generic parameter — answers `false`, which
/// is the conservative side: the rule then asks only for `Clone`.
fn type_is_provably_eq(type_text: &str) -> bool {
    type_text
        .split(|c: char| "<>,&*()[];".contains(c) || c.is_whitespace())
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .filter(|segment| !segment.starts_with('\''))
        .filter(|segment| !segment.chars().all(|c| c.is_ascii_digit()))
        .filter(|segment| !matches!(*segment, "const" | "mut"))
        .all(|segment| {
            PROVABLY_EQ_TYPES.contains(&segment.rsplit("::").next().unwrap_or(segment))
        })
}

/// True for the FFI-handle shape: `#[repr(transparent)]` over a raw pointer.
/// Both halves are required — a `#[repr(transparent)]` over a plain value is a
/// normal newtype, and a bare raw-pointer newtype is not the handle idiom.
fn is_transparent_pointer_handle(
    struct_item: tree_sitter::Node,
    source: &[u8],
    inner_text: &str,
) -> bool {
    let trimmed = inner_text.trim_start();
    (trimmed.starts_with("*const") || trimmed.starts_with("*mut"))
        && has_attribute_option(struct_item, source, "repr", "transparent")
}

/// True when an outer attribute applies a `derive` through `cfg_attr`. The
/// derived trait list then depends on the build configuration, which this check
/// cannot resolve, so the newtype is left alone rather than flagged on a guess.
fn has_conditional_derive(struct_item: tree_sitter::Node, source: &[u8]) -> bool {
    any_outer_attribute(struct_item, source, |text| {
        text.contains("cfg_attr") && text.contains("derive")
    })
}

/// The traits the newtype should gain, in the order they belong in a
/// `#[derive(…)]`. `Copy` satisfies the `Clone` requirement (a type cannot
/// derive `Copy` without `Clone`), and `Hash` is added only when `Eq` is already
/// there — hashing a type whose equality is partial breaks the
/// `a == b => hash(a) == hash(b)` contract.
///
fn missing_derives(derives: &[String]) -> Vec<&'static str> {
    let derived = |trait_name: &str| {
        derives
            .iter()
            .any(|entry| entry.rsplit("::").next().unwrap_or(entry).trim() == trait_name)
    };
    let mut missing = Vec::new();
    if !derived("Clone") && !derived("Copy") {
        missing.push("Clone");
    }
    if !derived("PartialEq") {
        missing.push("PartialEq");
    }
    if !derived("Eq") {
        missing.push("Eq");
    } else if !derived("Hash") {
        missing.push("Hash");
    }
    missing
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_bare_newtype() {
        let found = run("pub struct UserId(u64);");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("Clone, PartialEq, Eq"), "{}", found[0].message);
    }

    #[test]
    fn flags_newtype_deriving_only_debug() {
        assert_eq!(run("#[derive(Debug)]\npub struct Email(String);").len(), 1);
    }

    #[test]
    fn flags_pub_crate_newtype() {
        assert_eq!(run("pub(crate) struct Token(String);").len(), 1);
    }

    #[test]
    fn flags_newtype_with_pub_inner_field() {
        assert_eq!(run("pub struct Meters(pub u32);").len(), 1);
    }

    #[test]
    fn mentions_hash_only_when_eq_is_derived() {
        let found = run("#[derive(PartialEq, Eq)]\npub struct Id(u64);");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("Clone, Hash"), "{}", found[0].message);
    }

    #[test]
    fn omits_clone_when_copy_is_derived() {
        let found = run("#[derive(Copy, Clone, Debug)]\npub struct Id(u64);");
        assert_eq!(found.len(), 1);
        assert!(!found[0].message.contains("Clone"), "{}", found[0].message);
    }

    #[test]
    fn allows_newtype_with_every_derive() {
        assert!(run("#[derive(Clone, PartialEq, Eq, Hash)]\npub struct Id(u64);").is_empty());
    }

    #[test]
    fn allows_hash_missing_on_its_own() {
        assert!(run("#[derive(Clone, PartialEq, Eq)]\npub struct Id(u64);").is_empty());
    }

    #[test]
    fn allows_private_newtype() {
        assert!(run("struct Id(u64);").is_empty());
    }

    #[test]
    fn allows_named_field_struct() {
        assert!(run("pub struct Id { value: u64 }").is_empty());
    }

    #[test]
    fn allows_multi_field_tuple_struct() {
        assert!(run("pub struct Span(usize, usize);").is_empty());
    }

    #[test]
    fn allows_unit_struct() {
        assert!(run("pub struct Marker;").is_empty());
    }

    #[test]
    fn allows_generic_newtype() {
        assert!(run("pub struct Wrapper<T>(T);").is_empty());
    }

    #[test]
    fn allows_float_newtype() {
        assert!(run("pub struct Meters(f64);").is_empty());
    }

    #[test]
    fn allows_newtype_over_vec_of_floats() {
        assert!(run("pub struct Samples(Vec<f64>);").is_empty());
    }

    #[test]
    fn allows_newtype_over_trait_object() {
        assert!(run("pub struct Handler(Box<dyn Fn(u32)>);").is_empty());
    }

    #[test]
    fn allows_newtype_with_manual_clone_impl() {
        let src = "pub struct Id(u64);\nimpl Clone for Id { fn clone(&self) -> Self { Self(self.0) } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_newtype_with_manual_partial_eq_impl() {
        let src = "pub struct Id(u64);\nimpl PartialEq for Id { fn eq(&self, other: &Self) -> bool { self.0 == other.0 } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_transparent_raw_pointer_handle() {
        assert!(run("#[repr(transparent)]\npub struct Handle(*const u8);").is_empty());
    }

    #[test]
    fn flags_raw_pointer_newtype_without_repr_transparent() {
        assert_eq!(run("pub struct Handle(*const u8);").len(), 1);
    }

    #[test]
    fn allows_conditional_derive() {
        let src = "#[cfg_attr(feature = \"extra\", derive(Clone, PartialEq, Eq))]\npub struct Id(u64);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_newtype_in_test_module() {
        assert!(run("#[cfg(test)]\nmod tests { pub struct Id(u64); }").is_empty());
    }

    #[test]
    fn allows_newtype_gated_on_cfg_test() {
        assert!(run("#[cfg(test)]\npub struct Id(u64);").is_empty());
    }

    #[test]
    fn allows_newtype_over_an_unresolvable_type() {
        assert!(run("pub struct Override(Gitignore);").is_empty());
    }

    #[test]
    fn allows_newtype_over_a_wrapped_unresolvable_type() {
        assert!(run("pub struct Handle(Request<IncomingBody>);").is_empty());
    }

    #[test]
    fn flags_newtype_over_a_container_of_known_eq_types() {
        let found = run("pub struct Ids(Vec<u64>);");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("Clone, PartialEq, Eq"), "{}", found[0].message);
    }

    #[test]
    fn allows_newtype_over_a_resource_handle() {
        assert!(run("pub struct Shared(Mutex<u32>);").is_empty());
    }

    #[test]
    fn allows_path_qualified_derives() {
        assert!(run("#[derive(core::clone::Clone, PartialEq, Eq, Hash)]\npub struct Id(u64);").is_empty());
    }
}
