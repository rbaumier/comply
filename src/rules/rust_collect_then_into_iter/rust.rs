//! rust-collect-then-into-iter backend.
//!
//! Walks `call_expression` nodes whose function is
//! `<expr>.into_iter` and whose receiver expression is itself a
//! `call_expression` ending in `.collect::<Vec<_>>()`. Flags the chain
//! because the `Vec` is allocated only for `into_iter` to consume it
//! again — a no-op round-trip.
//!
//! The `collect` turbofish must name `Vec`. A `collect` without a
//! turbofish, or one that names another collection (`HashSet`,
//! `BTreeSet`, `IndexMap`, …), is left alone: those either cannot be
//! proven to be a `Vec` or carry semantics (dedup, ordering, keying)
//! that the round-trip preserves.
//!
//! The chain is also left alone when the owned iterator *escapes* its
//! scope — it is a `return`/function-tail value or a struct field
//! initializer — rather than being consumed locally. There the `Vec`
//! materialization is load-bearing: it breaks a borrow so the source's
//! owner can move into a downstream closure, or yields an owning
//! `IntoIter` of the right type for the escaping slot. An escaping
//! chain that immediately re-collects (`…into_iter().collect()`) is
//! still a genuine round-trip and is flagged.

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
        // The owned iterator is load-bearing when it escapes its scope
        // without being immediately re-collected.
        if !into_iter_recollected(node, source_bytes) && chain_escapes(node) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-collect-then-into-iter",
            format!(
                "`.collect::<Vec<_>>().{method}()` round-trips through a \
                 `Vec` for nothing. Drop both calls — the preceding chain \
                 is already an iterator."
            ),
            Severity::Warning,
        ));
    }
}

/// True when the `into_iter()` result is immediately re-collected —
/// `…collect::<Vec<_>>().into_iter().collect(…)`. That is a genuine
/// round-trip even when it escapes (e.g. returned), so it must still
/// flag. The shape: `node`'s parent is a `field_expression` whose
/// `value` is `node` and whose `field` is `collect`.
fn into_iter_recollected(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "field_expression" {
        return false;
    }
    if parent.child_by_field_name("value") != Some(node) {
        return false;
    }
    parent
        .child_by_field_name("field")
        .is_some_and(|f| f.utf8_text(source) == Ok("collect"))
}

/// True when the owned iterator (the `into_iter()` result plus any
/// downstream lazy adapters) escapes its local scope: it is a
/// `return`/function-tail value or a struct field initializer, rather
/// than being consumed locally (a `let` binding, a `for`/`while`/loop
/// subject, or a discarded `;` statement).
fn chain_escapes(node: tree_sitter::Node) -> bool {
    let outermost = chain_outermost(node);

    let mut current = outermost;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            // Escape positions.
            "return_expression" => return true,
            "field_initializer" | "shorthand_field_initializer" => return true,
            "block" => {
                // The implicit-return tail is the block's last named child
                // with no trailing `;` — i.e. the child is the expression
                // itself, not an `expression_statement`.
                let mut cursor = parent.walk();
                let last_named = parent.named_children(&mut cursor).last();
                return last_named == Some(current);
            }
            // Expression wrappers: keep climbing, the value still flows out.
            "arguments"
            | "call_expression"
            | "tuple_expression"
            | "reference_expression"
            | "type_cast_expression"
            | "try_expression"
            | "await_expression"
            | "parenthesized_expression"
            | "unary_expression" => {
                current = parent;
            }
            // Any other context (let, for/while/loop, expression_statement,
            // …) consumes the iterator locally — not an escape.
            _ => return false,
        }
    }
    false
}

/// Walks up from the `into_iter()` call to the outermost expression of
/// its method chain: while the parent is a `field_expression` whose
/// `value` is the current node, or a `call_expression` whose `function`
/// is the current node (the `.method().method()` continuation), the
/// last such node is the chain's outermost expression.
fn chain_outermost(node: tree_sitter::Node) -> tree_sitter::Node {
    let mut current = node;
    while let Some(parent) = current.parent() {
        let continues = match parent.kind() {
            "field_expression" => parent.child_by_field_name("value") == Some(current),
            "call_expression" => parent.child_by_field_name("function") == Some(current),
            _ => false,
        };
        if continues {
            current = parent;
        } else {
            break;
        }
    }
    current
}

/// True when `node` is a `<expr>.collect::<Vec<_>>()` call.
///
/// Requires the `generic_function` (turbofish) form whose first type
/// argument names `Vec`. A bare `.collect()` is not matched: without an
/// explicit type the collected type is inferred and cannot be proven to
/// be a `Vec`. Any other named collection (`HashSet`, `BTreeMap`, …) is
/// not matched either, since dropping the round-trip would change
/// semantics.
fn receiver_is_collect_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    // Only `.collect::<Vec<_>>()` — a `generic_function` carrying a turbofish.
    if func.kind() != "generic_function" {
        return false;
    }
    let Some(field_expr) = func.child_by_field_name("function") else {
        return false;
    };
    if field_expr.kind() != "field_expression" {
        return false;
    }
    let Some(field) = field_expr.child_by_field_name("field") else {
        return false;
    };
    if field.utf8_text(source).unwrap_or("") != "collect" {
        return false;
    }
    turbofish_names_vec(func, source)
}

/// True when the turbofish on `generic_function` names `Vec` as its
/// outermost type — `collect::<Vec<_>>()`, `collect::<Vec<String>>()`,
/// or a path-qualified `collect::<std::vec::Vec<_>>()` (final segment
/// `Vec`).
fn turbofish_names_vec(generic_function: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = generic_function.child_by_field_name("type_arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    let Some(first_type) = args.named_children(&mut cursor).next() else {
        return false;
    };
    // `Vec<_>` parses as `generic_type` whose `type` field is the name.
    let type_name = match first_type.kind() {
        "generic_type" => match first_type.child_by_field_name("type") {
            Some(name) => name,
            None => return false,
        },
        // A bare `Vec` without arguments would be a plain identifier.
        "type_identifier" | "scoped_type_identifier" => first_type,
        _ => return false,
    };
    let text = type_name.utf8_text(source).unwrap_or("");
    text == "Vec" || text.rsplit("::").next() == Some("Vec")
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
    fn flags_collect_vec_then_into_iter() {
        let source = "fn f() { let _: Vec<_> = it.collect::<Vec<_>>().into_iter().collect(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_collect_vec_with_concrete_arg() {
        let source = "fn f() { let _ = xs.iter().map(f).collect::<Vec<String>>().into_iter(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_plain_collect_then_into_iter() {
        // No turbofish: the collected type is inferred and cannot be proven
        // to be a `Vec`, so dropping the round-trip may change semantics.
        let source = "fn f() { let _: Vec<_> = it.collect().into_iter().collect(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_collect_hashset_then_into_iter() {
        // The HashSet deduplicates; the round-trip is meaningful (issue #3265).
        let source = "fn f() { let _ = xs.iter().map(f).collect::<HashSet<_>>().into_iter().collect::<Vec<_>>().join(\"|\"); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_collect_btreeset_then_into_iter() {
        let source = "fn f() { let _ = xs.into_iter().collect::<BTreeSet<_>>().into_iter(); }";
        assert!(run_on(source).is_empty());
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

    #[test]
    fn allows_escape_via_return_tail_before_move_closure() {
        // The submodule shape (issue #3715): the owned iterator escapes as
        // the function-tail value, and a downstream `move` closure captures
        // the owner. The `Vec` breaks the borrow — load-bearing.
        let source =
            "fn f() { Ok(Some(it.map(g).collect::<Vec<_>>().into_iter().map(move |n| X { n }))) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_escape_via_explicit_return() {
        let source = "fn f() -> std::vec::IntoIter<u8> { return xs.iter().map(g).collect::<Vec<_>>().into_iter(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_escape_via_struct_field_store() {
        // The ripgrep shape: the owned iterator is stored in a struct field
        // of owning `IntoIter` type.
        let source =
            "fn f() -> W { W { its: xs.iter().map(g).collect::<Vec<_>>().into_iter() } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_escape_via_bare_function_tail() {
        let source = "fn f() -> std::vec::IntoIter<u8> { xs.iter().map(g).collect::<Vec<_>>().into_iter() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_recollect_even_in_return_position() {
        // The owned iterator is immediately re-collected — a genuine
        // round-trip — even though the result escapes as the function tail.
        let source =
            "fn f() -> Vec<u8> { xs.iter().map(g).collect::<Vec<_>>().into_iter().collect() }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_for_loop_subject() {
        // Consumed locally by the loop — not an escape.
        let source = "fn f() { for x in xs.iter().map(g).collect::<Vec<_>>().into_iter() {} }";
        assert_eq!(run_on(source).len(), 1);
    }
}
