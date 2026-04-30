//! rust-collect-then-into-iter backend.
//!
//! Walks `call_expression` nodes whose function is
//! `<expr>.into_iter` and whose receiver expression is itself a
//! `call_expression` ending in `.collect`. Flags the chain because
//! the `collect` allocates a `Vec` (or similar) only for `into_iter`
//! to consume it again — a no-op round-trip.

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
        // We need `<receiver>.into_iter` as the function.
        let (receiver, method) = match func.kind() {
            "field_expression" => {
                let value = func.child_by_field_name("value");
                let field = func.child_by_field_name("field");
                let Some(field) = field else { return };
                let name = field.utf8_text(source_bytes).unwrap_or("");
                if name != "into_iter" {
                    return;
                }
                (value, name)
            }
            "generic_function" => {
                // `.into_iter::<...>()` is not the typical form, skip.
                return;
            }
            _ => return,
        };
        let Some(receiver) = receiver else { return };
        if !receiver_is_collect_call(receiver, source_bytes) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-collect-then-into-iter",
            format!(
                "`.collect::<...>().{method}()` round-trips through a \
                 `Vec` for nothing. Drop both calls — the preceding chain \
                 is already an iterator."
            ),
            Severity::Warning,
        ));
    }
}

fn receiver_is_collect_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    // `.collect()` (field_expression) or `.collect::<Vec<_>>()` (generic_function).
    let field_expr = match func.kind() {
        "field_expression" => func,
        "generic_function" => match func.child_by_field_name("function") {
            Some(inner) if inner.kind() == "field_expression" => inner,
            _ => return false,
        },
        _ => return false,
    };
    let Some(field) = field_expr.child_by_field_name("field") else {
        return false;
    };
    field.utf8_text(source).unwrap_or("") == "collect"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_collect_then_into_iter() {
        let source = "fn f() { let _: Vec<_> = it.collect::<Vec<_>>().into_iter().collect(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_plain_collect_then_into_iter() {
        let source = "fn f() { let _: Vec<_> = it.collect().into_iter().collect(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_collect_alone() {
        let source = "fn f() { let _: Vec<_> = it.collect(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_into_iter_on_vec_var() {
        let source = "fn f(v: Vec<u8>) { for x in v.into_iter() {} }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_other_method_after_collect() {
        let source = "fn f() { let n = it.collect::<Vec<_>>().len(); }";
        assert!(run_on(source).is_empty());
    }
}
