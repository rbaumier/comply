//! rust-partial-eq-without-eq backend.
//!
//! Walks every `struct_item` / `enum_item` and reads its outer
//! attributes plus any sibling `impl PartialEq for T` / `impl Eq
//! for T` blocks in the same file. If `PartialEq` is present
//! (derived or manually implemented) but `Eq` is missing, we emit
//! a diagnostic at the type definition.
//!
//! Files containing `f32` / `f64` field types are out of scope:
//! a struct holding a float legitimately can't implement `Eq`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["struct_item", "enum_item"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(type_name) = name_node.utf8_text(source_bytes) else {
            return;
        };
        // Skip types that hold floats — partial equality is correct there.
        if contains_float_field(node, source_bytes) {
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

fn contains_float_field(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    text.contains("f32") || text.contains("f64")
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
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
}
