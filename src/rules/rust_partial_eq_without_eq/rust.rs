//! rust-partial-eq-without-eq backend.
//!
//! Walks every `struct_item` / `enum_item` and reads its outer
//! attributes plus any sibling `impl PartialEq for T` / `impl Eq
//! for T` blocks in the same file. If `PartialEq` is present
//! (derived or manually implemented) but `Eq` is missing, we emit
//! a diagnostic at the type definition.
//!
//! Types whose fields are (or transitively wrap) `f32` / `f64` are
//! out of scope: floats are only `PartialEq`, so such a type cannot
//! implement `Eq`. Float detection walks the field-type AST for
//! `f32` / `f64` `primitive_type` nodes (covering arrays, tuples,
//! references and generic arguments) and resolves locally-defined
//! newtypes that themselves wrap a float.

use std::collections::HashSet;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["struct_item", "enum_item"];

/// Per-file memo of type names defined in the file whose fields are
/// (transitively) float-bearing. Computed once on the first visit.
type FloatTypeNames = Option<HashSet<String>>;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        let memo: FloatTypeNames = None;
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
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(type_name) = name_node.utf8_text(source_bytes) else {
            return;
        };
        let float_names = state.and_then(|s| s.downcast_mut::<FloatTypeNames>());
        // Skip types that hold floats — partial equality is correct there.
        if is_float_bearing_type(node, source_bytes, float_names) {
            return;
        }
        let derives = collect_derives(node, source_bytes);
        let (has_partial_eq, has_eq) =
            search_traits_in_root(node, source_bytes, type_name, &derives);
        if has_partial_eq && !has_eq {
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

/// Returns `true` when the type definition holds a float — either a
/// direct `f32` / `f64` field type, or a field whose type names a
/// locally-defined newtype that itself transitively wraps a float.
///
/// `float_names` memoizes the set of float-bearing local type names so
/// it is computed once per file rather than per visited type.
fn is_float_bearing_type(
    node: tree_sitter::Node,
    source: &[u8],
    float_names: Option<&mut FloatTypeNames>,
) -> bool {
    match float_names {
        Some(memo) => {
            let names = memo.get_or_insert_with(|| collect_float_type_names(node, source));
            type_def_has_float_field(node, source, names)
        }
        // No state available (defensive): fall back to direct floats only.
        None => type_def_has_float_field(node, source, &HashSet::new()),
    }
}

/// Whether any field type of the struct/enum is float-bearing given the
/// set of known float-bearing local type names.
fn type_def_has_float_field(
    node: tree_sitter::Node,
    source: &[u8],
    float_names: &HashSet<String>,
) -> bool {
    field_type_nodes(node)
        .iter()
        .any(|ty| type_node_is_float_bearing(*ty, source, float_names))
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

/// Whether a field-type AST node is float-bearing: it contains an
/// `f32` / `f64` `primitive_type` anywhere (arrays, tuples, references,
/// generic arguments) or names a known float-bearing local type.
fn type_node_is_float_bearing(
    ty: tree_sitter::Node,
    source: &[u8],
    float_names: &HashSet<String>,
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
                    && float_names.contains(text)
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
/// whose fields transitively wrap a float. A type is float-bearing if
/// any field is a direct `f32` / `f64`, or names another type already
/// known to be float-bearing.
fn collect_float_type_names(node: tree_sitter::Node, source: &[u8]) -> HashSet<String> {
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

    let mut float_names = HashSet::new();
    loop {
        let mut changed = false;
        for (name, def) in &defs {
            if float_names.contains(name) {
                continue;
            }
            if type_def_has_float_field(*def, source, &float_names) {
                float_names.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    float_names
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

/// Returns `(has_partial_eq, has_eq)` by combining derives + any
/// `impl Trait for TypeName` blocks at the file root.
fn search_traits_in_root(
    node: tree_sitter::Node,
    source: &[u8],
    type_name: &str,
    derives: &[String],
) -> (bool, bool) {
    let mut has_partial_eq = derives.iter().any(|d| d == "PartialEq");
    let mut has_eq = derives.iter().any(|d| d == "Eq");
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
                        has_partial_eq = true;
                    } else if bare == "Eq" {
                        has_eq = true;
                    }
                }
            }
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    (has_partial_eq, has_eq)
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
}
