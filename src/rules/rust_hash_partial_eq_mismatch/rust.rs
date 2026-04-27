//! rust-hash-partial-eq-mismatch backend.
//!
//! For each `struct_item` / `enum_item` we collect the source of
//! `Hash` and `PartialEq` (derive vs. manual `impl`). If one is
//! derived and the other is manually implemented, we flag the
//! definition. The Hash/Eq contract requires `a == b => hash(a) ==
//! hash(b)`; mixing derive and manual is the canonical way to
//! silently break it.

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
        let derives = collect_derives(node, source_bytes);
        let (manual_hash, manual_eq) = manual_impls(node, source_bytes, type_name);
        let derived_hash = derives.iter().any(|d| d == "Hash");
        let derived_eq = derives.iter().any(|d| d == "PartialEq");

        let has_hash = derived_hash || manual_hash;
        let has_eq = derived_eq || manual_eq;
        if !has_hash || !has_eq {
            return;
        }
        // Both are present; flag if their sources disagree.
        let mismatch = (derived_hash && manual_eq) || (manual_hash && derived_eq);
        if mismatch {
            diagnostics.push(Diagnostic::at_node(
                std::sync::Arc::clone(&ctx.path_arc),
                &name_node,
                "rust-hash-partial-eq-mismatch",
                format!(
                    "`{type_name}` mixes a derived and a manual implementation \
                     of `Hash` / `PartialEq`. Equal values must hash equal — \
                     either derive both or implement both manually."
                ),
                Severity::Error,
            ));
        }
    }
}

fn collect_derives(node: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(parent) = node.parent() else {
        return out;
    };
    let mut cursor = parent.walk();
    let children: Vec<_> = parent.children(&mut cursor).collect();
    let Some(idx) = children.iter().position(|c| c.id() == node.id()) else {
        return out;
    };
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
                for item in after[..end].split(',') {
                    out.push(item.trim().to_string());
                }
            }
        }
    }
    out
}

/// Returns `(has_manual_hash, has_manual_partial_eq)` for `type_name`
/// by walking the file root for `impl_item` blocks.
fn manual_impls(node: tree_sitter::Node, source: &[u8], type_name: &str) -> (bool, bool) {
    let mut root = node;
    while let Some(p) = root.parent() {
        root = p;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    let mut hash = false;
    let mut eq = false;
    while let Some(n) = stack.pop() {
        if n.kind() == "impl_item" {
            let trait_node = n.child_by_field_name("trait");
            let target_node = n.child_by_field_name("type");
            if let (Some(tr), Some(tg)) = (trait_node, target_node) {
                let trait_text = tr.utf8_text(source).unwrap_or("");
                let target_text = tg.utf8_text(source).unwrap_or("");
                if target_text == type_name {
                    let bare = trait_text.rsplit("::").next().unwrap_or(trait_text);
                    if bare == "Hash" {
                        hash = true;
                    } else if bare == "PartialEq" {
                        eq = true;
                    }
                }
            }
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    (hash, eq)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_derived_hash_manual_partial_eq() {
        let source = "#[derive(Hash)]\nstruct A;\n\
                      impl PartialEq for A { fn eq(&self, _: &Self) -> bool { true } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_manual_hash_derived_partial_eq() {
        let source = "#[derive(PartialEq)]\nstruct A;\n\
                      impl std::hash::Hash for A { fn hash<H: std::hash::Hasher>(&self, _: &mut H) {} }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_both_derived() {
        let source = "#[derive(Hash, PartialEq, Eq)]\nstruct A;";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_both_manual() {
        let source = "struct A;\n\
                      impl PartialEq for A { fn eq(&self, _: &Self) -> bool { true } }\n\
                      impl std::hash::Hash for A { fn hash<H: std::hash::Hasher>(&self, _: &mut H) {} }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_only_one() {
        let source = "#[derive(PartialEq)]\nstruct A;";
        assert!(run_on(source).is_empty());
    }
}
