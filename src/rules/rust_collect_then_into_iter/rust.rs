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
                "`.collect::<Vec<_>>().{method}()` round-trips through a \
                 `Vec` for nothing. Drop both calls — the preceding chain \
                 is already an iterator."
            ),
            Severity::Warning,
        ));
    }
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
}
