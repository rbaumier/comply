//! rust-iter-count-vs-len backend.
//!
//! Walks `call_expression` nodes whose function is `<expr>.count`
//! and whose receiver is itself a call ending in `.iter` or
//! `.iter_mut`. Flags the chain — `.iter().count()` walks the
//! whole collection in O(n) when `.len()` is O(1).
//!
//! We don't try to verify the receiver type; the heuristic that
//! "any `.iter().count()` chain is suspicious" matches what
//! clippy's `needless_collect`/`iter_count` family covers.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

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
        let source_bytes = ctx.source.as_bytes();
        let Some(func) = node.child_by_field_name("function") else {
            return;
        };
        if func.kind() != "field_expression" {
            return;
        }
        let Some(field) = func.child_by_field_name("field") else {
            return;
        };
        if field.utf8_text(source_bytes).unwrap_or("") != "count" {
            return;
        }
        let Some(receiver) = func.child_by_field_name("value") else {
            return;
        };
        if !receiver_is_iter_call(receiver, source_bytes) {
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

fn receiver_is_iter_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "field_expression" {
        return false;
    }
    let Some(field) = func.child_by_field_name("field") else {
        return false;
    };
    let name = field.utf8_text(source).unwrap_or("");
    name == "iter" || name == "iter_mut"
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
    fn flags_iter_count() {
        let source = "fn f(v: Vec<u8>) { let _ = v.iter().count(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_iter_mut_count() {
        let source = "fn f(v: &mut Vec<u8>) { let _ = v.iter_mut().count(); }";
        assert_eq!(run_on(source).len(), 1);
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
