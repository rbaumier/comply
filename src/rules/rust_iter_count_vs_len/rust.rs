//! rust-iter-count-vs-len backend.
//!
//! Walks `call_expression` nodes whose function is `<expr>.count` and whose
//! receiver is itself a call ending in `.iter` or `.iter_mut`. Flags the chain
//! only when the collection receiver is a confirmable std collection that
//! exposes `len()` — `.iter().count()` walks it in O(n) where `.len()` is O(1).
//!
//! `.iter()` is an inherent method any type may define returning a custom
//! adapter (graphs, schemas, trees) with no `len()`, so `<recv>.len()` would not
//! compile there. We therefore only flag when the receiver is confirmably one of
//! the std collections that has `len()`: a `vec![...]` / array literal, a local
//! `let`-bound `Vec`, or a binding (`let` or parameter) annotated with a known
//! collection type (`Vec<...>`, `&[...]`/`&mut [...]` slices, `[T; N]` arrays,
//! `VecDeque`/`HashMap`/`BTreeMap`/`HashSet`/`BTreeSet`/`BinaryHeap`/`LinkedList`,
//! optionally behind `&`/`&mut`). When the receiver's collection-ness cannot be
//! confirmed locally, we stay silent.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{find_identifier_type, local_let_binds_vec};

const KINDS: &[&str] = &["call_expression"];

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
        let source = ctx.source.as_bytes();
        let Some(func) = node.child_by_field_name("function") else {
            return;
        };
        if func.kind() != "field_expression" {
            return;
        }
        let Some(field) = func.child_by_field_name("field") else {
            return;
        };
        if field.utf8_text(source).unwrap_or("") != "count" {
            return;
        }
        let Some(iter_call) = func.child_by_field_name("value") else {
            return;
        };
        let Some(collection) = iter_call_receiver(iter_call, source) else {
            return;
        };
        if !receiver_is_confirmable_collection(collection, source) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-iter-count-vs-len",
            "`.iter().count()` walks the whole collection. Use `.len()` \
             directly on the collection (O(1) vs O(n))."
                .into(),
            Severity::Warning,
        ));
    }
}

/// If `node` is a `<recv>.iter()` / `<recv>.iter_mut()` call, return the receiver
/// expression node (`<recv>`). Returns `None` for any other call, including
/// `into_iter` (which consumes — `.len()` is not an equivalent rewrite there).
fn iter_call_receiver<'a>(node: tree_sitter::Node<'a>, source: &[u8]) -> Option<tree_sitter::Node<'a>> {
    if node.kind() != "call_expression" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    if func.kind() != "field_expression" {
        return None;
    }
    let field = func.child_by_field_name("field")?;
    let name = field.utf8_text(source).unwrap_or("");
    if name != "iter" && name != "iter_mut" {
        return None;
    }
    func.child_by_field_name("value")
}

/// True if `receiver` is confirmably a std collection that exposes `len()`:
///
/// - a `vec![...]` macro or array `[...]` literal;
/// - a plain identifier resolving to a local `let`-bound `Vec`, or to a binding
///   (`let` or parameter) whose type annotation is a known collection
///   ([`type_text_is_std_collection`]).
///
/// Any other receiver — a custom `iter()`, a field access, a method-chain
/// receiver, or an identifier whose type cannot be resolved from the AST — is
/// not confirmed, so the rule stays silent.
fn receiver_is_confirmable_collection(receiver: tree_sitter::Node, source: &[u8]) -> bool {
    match receiver.kind() {
        "macro_invocation" => receiver
            .child_by_field_name("macro")
            .and_then(|m| m.utf8_text(source).ok())
            == Some("vec"),
        "array_expression" => true,
        "identifier" => {
            let Ok(name) = receiver.utf8_text(source) else {
                return false;
            };
            if local_let_binds_vec(receiver, name, source) {
                return true;
            }
            find_identifier_type(receiver, name, source)
                .is_some_and(|ty| type_text_is_std_collection(&ty))
        }
        _ => false,
    }
}

/// True if `ty` (a binding's type-annotation source text) names a std collection
/// that exposes `len()`, optionally behind one `&`/`&mut` borrow:
/// `Vec<...>`, a `&[...]`/`&mut [...]` slice, a `[T; N]` array, or one of
/// `VecDeque`/`HashMap`/`BTreeMap`/`HashSet`/`BTreeSet`/`BinaryHeap`/`LinkedList`.
fn type_text_is_std_collection(ty: &str) -> bool {
    let ty = ty.trim();
    let ty = ty
        .strip_prefix("&mut ")
        .or_else(|| ty.strip_prefix('&'))
        .unwrap_or(ty)
        .trim_start();
    if ty.starts_with('[') {
        // `[T]` slice or `[T; N]` array — both expose `len()`.
        return true;
    }
    const COLLECTIONS: &[&str] = &[
        "Vec",
        "VecDeque",
        "HashMap",
        "BTreeMap",
        "HashSet",
        "BTreeSet",
        "BinaryHeap",
        "LinkedList",
    ];
    // Match the last path segment so `std::vec::Vec<...>` / `collections::HashMap<...>`
    // are recognized, while a custom `MyVec<...>` is not.
    let head = ty.split('<').next().unwrap_or(ty).trim();
    let last_segment = head.rsplit("::").next().unwrap_or(head).trim();
    ty.contains('<') && COLLECTIONS.contains(&last_segment)
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
    fn flags_iter_count_on_vec_param() {
        let source = "fn f(v: Vec<u8>) { let _ = v.iter().count(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_iter_count_on_slice_param() {
        let source = "fn f(v: &[u32]) -> usize { v.iter().count() }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_iter_count_on_local_vec() {
        let source = "fn f() { let v: Vec<u32> = Vec::new(); let _ = v.iter().count(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_iter_mut_count() {
        let source = "fn f(v: &mut Vec<u8>) { let _ = v.iter_mut().count(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_iter_count_on_vec_literal() {
        let source = "fn f() { let _ = vec![1, 2, 3].iter().count(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_iter_count_on_hashmap_param() {
        let source = "fn f(m: HashMap<u32, u32>) { let _ = m.iter().count(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_iter_count_on_array_literal() {
        let source = "fn f() { let _ = [1, 2, 3].iter().count(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    // DFSchema-shaped false positive (issue #3804): a custom `iter()` on a type
    // with no `len()`. `s` is a `&Schema`, not a confirmable std collection, so
    // `s.len()` would not compile — stay silent.
    #[test]
    fn allows_custom_iter_with_no_len_issue_3804() {
        let source = "fn width(s: &Schema) -> usize { s.iter().count() }";
        assert!(run_on(source).is_empty());
    }

    // The exact DFSchema shape from the issue: `schema: &Arc<DFSchema>` resolves
    // to a type, but `Arc<DFSchema>` is not a std collection with `len()`, so the
    // suggested `schema.len()` would not compile — stay silent.
    #[test]
    fn allows_arc_wrapped_receiver_issue_3804() {
        let source = "fn f(schema: &Arc<DFSchema>) -> usize { schema.iter().count() }";
        assert!(run_on(source).is_empty());
    }

    // An identifier with no resolvable binding (no annotation, no local `Vec`)
    // is left alone.
    #[test]
    fn allows_unconfirmed_receiver() {
        let source = "fn f() -> usize { schema.iter().count() }";
        assert!(run_on(source).is_empty());
    }

    // A field-access receiver can't be type-checked from the AST.
    #[test]
    fn allows_field_receiver() {
        let source = "fn f(&self) -> usize { self.schema.iter().count() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_filter_count() {
        let source = "fn f(v: Vec<u8>) { let _ = v.iter().filter(|x| **x > 0).count(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_len_directly() {
        let source = "fn f(v: Vec<u8>) { let _ = v.len(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_into_iter_count() {
        // into_iter consumes — `.len()` would not be equivalent.
        let source = "fn f(v: Vec<u8>) { let _ = v.into_iter().count(); }";
        assert!(run_on(source).is_empty());
    }
}
