//! rust-no-large-tuple-return backend.
//!
//! Walks `function_item` nodes whose return type is a `tuple_type`
//! with 3 or more positional element types. Two-element tuples are
//! a borderline case (key/value pairs are common) so we leave them
//! alone — three is the threshold where named fields start paying
//! for themselves.
//!
//! Five exemptions suppress an otherwise-flagged function:
//! - Trait-impl methods: the tuple return type is fixed by the trait
//!   contract, so the implementor cannot swap it for a named struct.
//! - Value decompositions: a tuple whose every element is a
//!   lightweight type (primitive, enum, reference, bare struct name)
//!   hands back the components of one value the way `as_hms` returns
//!   `(u8, u8, u8)`. A named struct adds ceremony without intent. One
//!   owning heap container (`String`, `Vec<_>`, a map, …) makes it a
//!   bundle again and the function flags.
//! - Private positional returns: a non-`pub` function whose tuple
//!   elements are all textually identical, or all name the function's
//!   own generic type parameters — named fields add no information
//!   there. Tuples mixing distinct concrete types still flag.
//! - An enclosing `#[allow(clippy::type_complexity)]` /
//!   `#[expect(clippy::type_complexity)]`: this rule overlaps that
//!   canonical clippy lint, so an author who has silenced it has
//!   already made the call.
//! - Test-context functions (`#[test]` / `#[cfg(test)]` mods or
//!   `#![cfg(test)]` files): returning an RAII guard plus the
//!   endpoints under test as a tuple is the textbook fixture shape
//!   for code that never ships, so a named struct is pure ceremony.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{
    has_clippy_allow, is_in_test_context, is_in_trait_impl, is_pub, trait_base_name,
};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["function_item"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let max_elements = ctx
            .config
            .threshold("rust-no-large-tuple-return", "max_elements", ctx.lang);
        let Some(ret_type) = node.child_by_field_name("return_type") else {
            return;
        };
        if ret_type.kind() != "tuple_type" {
            return;
        }
        let mut cursor = ret_type.walk();
        let count = ret_type.named_children(&mut cursor).count();
        if count < max_elements {
            return;
        }
        if is_in_trait_impl(node) {
            return;
        }
        if tuple_is_value_decomposition(ret_type, source_bytes) {
            return;
        }
        if !is_pub(node, source_bytes) && tuple_is_positional(ret_type, node, source_bytes) {
            return;
        }
        if has_clippy_allow(node, source_bytes, "type_complexity") {
            return;
        }
        if is_in_test_context(node, source_bytes) {
            return;
        }
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_bytes).ok())
            .unwrap_or("f");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-large-tuple-return".into(),
            message: format!(
                "`fn {name}` returns a {count}-element tuple — wrap \
                 the result in a named struct so each field has a \
                 name and refactors don't break every caller."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Owning heap containers whose presence turns a tuple into a bundle
/// that wants a named field. Borrowed views, primitives, enums and
/// other lightweight types are decomposition components, not bundles.
const OWNING_CONTAINERS: &[&str] = &[
    "String",
    "Vec",
    "Box",
    "Rc",
    "Arc",
    "Cow",
    "Cell",
    "RefCell",
    "Mutex",
    "RwLock",
    "VecDeque",
    "BinaryHeap",
    "HashMap",
    "BTreeMap",
    "HashSet",
    "BTreeSet",
];

/// A tuple return is a value decomposition — the idiomatic way to hand
/// back the scalar components of one value (`as_hms() -> (u8, u8, u8)`,
/// `to_calendar_date() -> (i32, Month, u8)`) — when every element is a
/// lightweight type. Naming the fields of such a tuple adds ceremony
/// without intent: the position already documents the component.
///
/// An element is heavy, and the tuple a bundle worth a named struct,
/// when it is an owning heap container (`String`, `Vec<_>`, `Box<_>`,
/// a map/set, …). One heavy element disqualifies the whole tuple.
///
/// The AST carries no type information, so a bare name like `Month` is
/// indistinguishable from a large owned struct. The exemption errs
/// toward silence: a tuple of bare named types reads as a
/// decomposition. Heap containers are the one element shape nameable
/// with confidence, so they alone keep a tuple flagged.
fn tuple_is_value_decomposition(ret_type: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = ret_type.walk();
    ret_type
        .named_children(&mut cursor)
        .all(|element| !element_is_heavy(element, source))
}

/// True when `element` is an owning heap container — the only tuple
/// element that keeps a return type flagged under the decomposition
/// exemption. References, primitives and bare enum/struct names are
/// lightweight components, never heavy.
fn element_is_heavy(element: tree_sitter::Node, source: &[u8]) -> bool {
    trait_base_name(element, source).is_some_and(|base| OWNING_CONTAINERS.contains(&base))
}

/// A tuple is positional when naming its fields would add nothing:
/// either every element type is textually identical, or every element
/// names one of the function's own generic type parameters.
fn tuple_is_positional(
    ret_type: tree_sitter::Node,
    fn_node: tree_sitter::Node,
    source: &[u8],
) -> bool {
    let mut cursor = ret_type.walk();
    let elements: Vec<tree_sitter::Node> = ret_type.named_children(&mut cursor).collect();
    let texts: Vec<&str> = elements
        .iter()
        .filter_map(|e| e.utf8_text(source).ok())
        .collect();
    if texts.len() != elements.len() {
        return false;
    }
    if texts.windows(2).all(|pair| pair[0] == pair[1]) {
        return true;
    }
    let params = generic_param_names(fn_node, source);
    elements
        .iter()
        .zip(&texts)
        .all(|(element, text)| element.kind() == "type_identifier" && params.contains(text))
}

fn generic_param_names<'a>(fn_node: tree_sitter::Node, source: &'a [u8]) -> Vec<&'a str> {
    let Some(type_params) = fn_node.child_by_field_name("type_parameters") else {
        return Vec::new();
    };
    let mut cursor = type_params.walk();
    type_params
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "type_parameter")
        .filter_map(|child| child.child_by_field_name("name"))
        .filter_map(|name| name.utf8_text(source).ok())
        .collect()
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
    fn flags_three_element_tuple_return() {
        assert_eq!(
            run_on("fn parse() -> (String, i32, bool) { todo!() }").len(),
            1
        );
    }

    #[test]
    fn flags_four_element_tuple_return() {
        assert_eq!(
            run_on("fn parse() -> (String, i32, bool, Vec<u8>) { todo!() }").len(),
            1
        );
    }

    #[test]
    fn allows_pair_tuple_return() {
        assert!(run_on("fn split() -> (String, String) { todo!() }").is_empty());
    }

    #[test]
    fn allows_named_struct_return() {
        assert!(run_on("fn parse() -> ParseResult { todo!() }").is_empty());
    }

    #[test]
    fn allows_trait_impl_method() {
        assert!(run_on(
            "struct C; impl Consumer for C { \
             fn split_at(self, index: usize) -> (Self, Self, Self::Reducer) { todo!() } }"
        )
        .is_empty());
    }

    #[test]
    fn flags_inherent_impl_method() {
        assert_eq!(
            run_on(
                "struct C; impl C { \
                 fn split_at(self, index: usize) -> (String, i32, bool) { todo!() } }"
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_private_same_type_tuple_return() {
        assert!(run_on(
            "fn quarter_chunks(v: &[f32]) -> (&[f32], &[f32], &[f32], &[f32]) { todo!() }"
        )
        .is_empty());
    }

    #[test]
    fn allows_private_generic_param_tuple_return() {
        assert!(run_on(
            "fn join4<R1, R2, R3, R4>(a: R1, b: R2, c: R3, d: R4) -> (R1, R2, R3, R4) { todo!() }"
        )
        .is_empty());
    }

    #[test]
    fn allows_public_primitive_decomposition() {
        assert!(run_on("pub const fn as_hms(self) -> (u8, u8, u8) { todo!() }").is_empty());
    }

    #[test]
    fn allows_public_four_primitive_decomposition() {
        assert!(
            run_on("pub const fn as_hms_milli(self) -> (u8, u8, u8, u16) { todo!() }").is_empty()
        );
    }

    #[test]
    fn allows_primitive_enum_mixed_decomposition() {
        assert!(
            run_on("pub const fn to_calendar_date(self) -> (i32, Month, u8) { todo!() }")
                .is_empty()
        );
        assert!(
            run_on("pub const fn to_iso_week_date(self) -> (i32, u8, Weekday) { todo!() }")
                .is_empty()
        );
    }

    #[test]
    fn flags_public_bundle_with_owning_container() {
        assert_eq!(run_on("pub fn f() -> (String, i32, bool) { todo!() }").len(), 1);
        assert_eq!(
            run_on("pub fn f() -> (String, i32, bool, Vec<u8>) { todo!() }").len(),
            1
        );
    }

    #[test]
    fn flags_qualified_owning_container_bundle() {
        assert_eq!(
            run_on("pub fn f() -> (i32, std::collections::HashMap<String, i32>, bool) { todo!() }")
                .len(),
            1
        );
    }

    #[test]
    fn allows_bare_named_types_decomposition() {
        assert!(run_on("pub fn f() -> (Foo, Bar, Baz) { todo!() }").is_empty());
    }

    #[test]
    fn allows_clippy_type_complexity_allow() {
        assert!(run_on(
            "#[allow(clippy::type_complexity)] \
             pub fn f() -> (String, i32, bool) { todo!() }"
        )
        .is_empty());
    }

    #[test]
    fn allows_clippy_type_complexity_expect() {
        assert!(run_on(
            "#[expect(clippy::type_complexity)] \
             pub fn f() -> (String, i32, bool) { todo!() }"
        )
        .is_empty());
    }

    #[test]
    fn allows_clippy_type_complexity_grouped_allow() {
        assert!(run_on(
            "#[allow(clippy::type_complexity, dead_code)] \
             pub fn f() -> (String, i32, bool) { todo!() }"
        )
        .is_empty());
    }

    #[test]
    fn flags_unrelated_allow() {
        assert_eq!(
            run_on(
                "#[allow(dead_code)] \
                 pub fn f() -> (String, i32, bool) { todo!() }"
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_tuple_return_in_cfg_test_mod() {
        assert!(run_on(
            "#[cfg(test)]\nmod tests {\n    \
             fn make_ring() -> (AlignedRegion, DsmMpscReceiver, DsmMpscSender) { todo!() }\n}"
        )
        .is_empty());
    }

    #[test]
    fn allows_tuple_return_under_test_fn() {
        assert!(run_on(
            "#[test]\nfn t() {\n    \
             fn make_ring() -> (A, B, C) { todo!() }\n}"
        )
        .is_empty());
    }
}
