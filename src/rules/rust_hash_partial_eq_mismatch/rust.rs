//! rust-hash-partial-eq-mismatch backend.
//!
//! For each `struct_item` / `enum_item` we collect the source of
//! `Hash` and `PartialEq` (derive vs. manual `impl`). We flag only a
//! derived `Hash` paired with a manual `PartialEq`: a derived `Hash`
//! reads every field, so a manual `eq` that ignores one can make
//! `a == b` while `hash(a) != hash(b)`, breaking the `a == b =>
//! hash(a) == hash(b)` contract. The reverse mix — a manual `Hash`
//! with a derived `PartialEq` — is the idiomatic subset-hash
//! optimization: derived `PartialEq` compares all fields, so equal
//! values agree on the subset the manual `Hash` reads and hash equal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::collect_top_level_derives;

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
        let derives = collect_top_level_derives(node, source_bytes);
        let (manual_hash, manual_eq) = manual_impls(node, source_bytes, type_name);
        let derived_hash = derives.iter().any(|d| d == "Hash");
        let derived_eq = derives.iter().any(|d| d == "PartialEq");

        let has_hash = derived_hash || manual_hash;
        let has_eq = derived_eq || manual_eq;
        if !has_hash || !has_eq {
            return;
        }
        // Both are present; flag only the contract-breaking direction:
        // a derived `Hash` (reads all fields) with a manual `PartialEq`
        // that may ignore some. A manual `Hash` over a subset of the
        // derived-`PartialEq` fields is the safe idiomatic optimization.
        let mismatch = derived_hash && manual_eq;
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
    fn flags_derived_hash_manual_partial_eq() {
        let source = "#[derive(Hash)]\nstruct A;\n\
                      impl PartialEq for A { fn eq(&self, _: &Self) -> bool { true } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_manual_hash_derived_partial_eq() {
        // A manual `Hash` with a derived `PartialEq` is the idiomatic
        // subset-hash optimization: derived `PartialEq` compares every
        // field, so equal values agree on the subset the manual `Hash`
        // reads and hash equal. Mirrors datafusion's `Alias` (#3806).
        let source = "#[derive(PartialEq, Eq)]\n\
                      struct Key { id: u64, cached_label: String }\n\
                      impl std::hash::Hash for Key { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { self.id.hash(state); } }";
        assert!(run_on(source).is_empty());
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

    #[test]
    fn ignores_derive_nested_in_cfg_attr_rkyv() {
        // `rkyv(derive(...))` generates impls on the archived companion type,
        // not on `Version`; `Version` itself implements Hash/PartialEq/Eq
        // manually and consistently. The nested `derive(` must not be read as a
        // derive on the host. Reproduces astral-sh/uv version.rs:277 (#3944).
        let source = "#[derive(Clone)]\n\
                      #[cfg_attr(feature = \"rkyv\", rkyv(derive(Debug, Eq, PartialEq, PartialOrd, Ord)))]\n\
                      pub struct Version { inner: u32 }\n\
                      impl PartialEq for Version { fn eq(&self, _o: &Self) -> bool { true } }\n\
                      impl Eq for Version {}\n\
                      impl std::hash::Hash for Version { fn hash<H: std::hash::Hasher>(&self, _s: &mut H) {} }\n\
                      impl PartialOrd for Version { fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(o)) } }\n\
                      impl Ord for Version { fn cmp(&self, _o: &Self) -> std::cmp::Ordering { std::cmp::Ordering::Equal } }";
        assert!(run_on(source).is_empty());
    }
}
