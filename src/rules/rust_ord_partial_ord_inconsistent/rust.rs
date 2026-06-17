//! rust-ord-partial-ord-inconsistent backend.
//!
//! Same shape as `rust-hash-partial-eq-mismatch` but for the
//! `Ord` / `PartialOrd` pair. The Ord/PartialOrd contract requires
//! `partial_cmp` to delegate to `cmp` when both are present;
//! mixing derive and manual is the standard way to violate it.

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
        let (manual_ord, manual_partial_ord) = manual_impls(node, source_bytes, type_name);
        let derived_ord = derives.iter().any(|d| d == "Ord");
        let derived_partial_ord = derives.iter().any(|d| d == "PartialOrd");

        let has_ord = derived_ord || manual_ord;
        let has_partial_ord = derived_partial_ord || manual_partial_ord;
        if !has_ord || !has_partial_ord {
            return;
        }
        let mismatch = (derived_ord && manual_partial_ord) || (manual_ord && derived_partial_ord);
        if mismatch {
            diagnostics.push(Diagnostic::at_node(
                std::sync::Arc::clone(&ctx.path_arc),
                &name_node,
                "rust-ord-partial-ord-inconsistent",
                format!(
                    "`{type_name}` mixes derived and manual implementations of \
                     `Ord` / `PartialOrd`. The two must agree: \
                     `partial_cmp` should delegate to `cmp`. Either derive both \
                     or implement both manually."
                ),
                Severity::Error,
            ));
        }
    }
}

fn manual_impls(node: tree_sitter::Node, source: &[u8], type_name: &str) -> (bool, bool) {
    let mut root = node;
    while let Some(p) = root.parent() {
        root = p;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    let mut ord = false;
    let mut partial_ord = false;
    while let Some(n) = stack.pop() {
        if n.kind() == "impl_item" {
            let trait_node = n.child_by_field_name("trait");
            let target_node = n.child_by_field_name("type");
            if let (Some(tr), Some(tg)) = (trait_node, target_node) {
                let trait_text = tr.utf8_text(source).unwrap_or("");
                let target_text = tg.utf8_text(source).unwrap_or("");
                if target_text == type_name {
                    let bare = trait_text.rsplit("::").next().unwrap_or(trait_text);
                    if bare == "Ord" {
                        ord = true;
                    } else if bare == "PartialOrd" {
                        partial_ord = true;
                    }
                }
            }
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    (ord, partial_ord)
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
    fn flags_derived_ord_manual_partial_ord() {
        let source = "#[derive(Ord, PartialEq, Eq)]\nstruct A;\n\
                      impl PartialOrd for A { fn partial_cmp(&self, _: &Self) \
                      -> Option<std::cmp::Ordering> { Some(std::cmp::Ordering::Equal) } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_manual_ord_derived_partial_ord() {
        let source = "#[derive(PartialOrd, PartialEq, Eq)]\nstruct A;\n\
                      impl Ord for A { fn cmp(&self, _: &Self) -> std::cmp::Ordering { std::cmp::Ordering::Equal } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_both_derived() {
        let source = "#[derive(Ord, PartialOrd, PartialEq, Eq)]\nstruct A;";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_both_manual() {
        let source = "struct A;\n\
                      impl Ord for A { fn cmp(&self, _: &Self) -> std::cmp::Ordering { std::cmp::Ordering::Equal } }\n\
                      impl PartialOrd for A { fn partial_cmp(&self, _: &Self) \
                      -> Option<std::cmp::Ordering> { Some(std::cmp::Ordering::Equal) } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_only_partial_ord() {
        let source = "#[derive(PartialOrd, PartialEq)]\nstruct A { x: f64 }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_derive_nested_in_cfg_attr_rkyv() {
        // `rkyv(derive(...))` generates impls on the archived companion type,
        // not on `Version`; `Version` itself implements Ord/PartialOrd manually
        // and consistently. The nested `derive(` must not be read as a derive
        // on the host. Reproduces astral-sh/uv version.rs:277 (issue #3944).
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
