//! rust-loop-collect-into-existing-vec backend.
//!
//! Match `for_expression` whose body is a single-statement block calling
//! `<receiver>.push(...)` where `<receiver>` is a local binding confirmed to
//! be a `Vec`. We do not require the push argument to be the loop variable —
//! when the push value is infallible, `for x in src { v.push(transform(x)); }`
//! is still better written as `v.extend(src.into_iter().map(transform))`.
//!
//! A push argument containing `?` (a `try_expression`) is skipped: the `?`
//! propagates to the enclosing function, and a fallible value cannot be lifted
//! into a `map` closure (the `?` would return from the closure, not the
//! function), so `extend(...map(...))` would not compile.
//!
//! `.push` exists on many non-`Vec` types (`VecDeque`, crossbeam `Worker`,
//! `Injector`, custom queues), none of which `extend`s the same way, so we
//! only flag when the receiver is an in-scope `let` whose initializer is
//! `Vec`-shaped (`Vec::new()`, `Vec::with_capacity(...)`, `vec![...]`) or
//! that carries an explicit `: Vec<...>` annotation. When the receiver's
//! `Vec`-ness cannot be confirmed locally, we stay silent.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["for_expression"];

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
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        if body.kind() != "block" {
            return;
        }
        // Single-statement body only — multi-statement loops can have
        // side effects we can't summarize as `extend`.
        let mut cursor = body.walk();
        let stmts: Vec<_> = body.named_children(&mut cursor).collect();
        if stmts.len() != 1 {
            return;
        }
        let stmt = stmts[0];
        let call = match stmt.kind() {
            "expression_statement" => {
                let mut c = stmt.walk();
                stmt.named_children(&mut c).next()
            }
            "call_expression" => Some(stmt),
            _ => None,
        };
        let Some(call) = call else { return };
        if call.kind() != "call_expression" {
            return;
        }
        let Some(function) = call.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "field_expression" {
            return;
        }
        let Some(field) = function.child_by_field_name("field") else {
            return;
        };
        let Ok(method) = field.utf8_text(source) else {
            return;
        };
        if method != "push" {
            return;
        }
        // A push argument that uses `?` propagates an error to the enclosing
        // function; it can't be lifted into a `map` closure, so the
        // `extend(...map(...))` rewrite would not compile. Skip it.
        if let Some(arguments) = call.child_by_field_name("arguments")
            && arg_contains_try(arguments)
        {
            return;
        }
        // Only flag when the receiver is a plain local identifier we can
        // resolve to a confirmable `Vec`. A field access (`self.q.push`) or
        // method chain receiver can't be checked, so it is left untouched.
        let Some(receiver) = function.child_by_field_name("value") else {
            return;
        };
        if receiver.kind() != "identifier" {
            return;
        }
        let Ok(var) = receiver.utf8_text(source) else {
            return;
        };
        if !receiver_is_local_vec(node, var, source) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-loop-collect-into-existing-vec",
            "`for x in src { dst.push(...); }` is `dst.extend(src.into_iter().map(...))`. \
             `extend` reserves capacity from `size_hint`; the loop reallocates per element."
                .into(),
            Severity::Warning,
        ));
    }
}

/// Whether the subtree rooted at `node` contains a `?` (`try_expression`)
/// anywhere. Used on the `push` argument list: a fallible push value cannot be
/// expressed as `extend(...map(...))`.
fn arg_contains_try(node: tree_sitter::Node) -> bool {
    if node.kind() == "try_expression" {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(arg_contains_try)
}

/// Walk up the enclosing scopes from `for_node` looking for a `let`
/// declaration that binds `var` to a confirmable `Vec`. Only declarations
/// that lexically precede the loop in their block are considered.
fn receiver_is_local_vec(for_node: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    let mut child = for_node;
    while let Some(parent) = child.parent() {
        let mut cursor = parent.walk();
        for sib in parent.children(&mut cursor) {
            if sib.id() == child.id() {
                break;
            }
            if sib.kind() == "let_declaration" && let_binds_vec(sib, var, source) {
                return true;
            }
        }
        child = parent;
    }
    false
}

/// Whether `let_node` declares `var` with a `Vec`-shaped initializer or an
/// explicit `Vec<...>` type annotation.
fn let_binds_vec(let_node: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    let Some(pattern) = let_node.child_by_field_name("pattern") else {
        return false;
    };
    if !pattern_binds(pattern, var, source) {
        return false;
    }
    if let Some(ty) = let_node.child_by_field_name("type")
        && ty.utf8_text(source).unwrap_or("").trim_start().starts_with("Vec<")
    {
        return true;
    }
    if let Some(value) = let_node.child_by_field_name("value") {
        let text = value.utf8_text(source).unwrap_or("");
        if text.starts_with("Vec::") || text.starts_with("vec!") {
            return true;
        }
    }
    false
}

/// Whether a `let` pattern (`x` or `mut x`) binds the name `var`.
fn pattern_binds(pattern: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    let name = match pattern.kind() {
        "identifier" => pattern.utf8_text(source).ok(),
        "mut_pattern" => {
            let mut cursor = pattern.walk();
            pattern
                .children(&mut cursor)
                .find(|c| c.kind() == "identifier")
                .and_then(|c| c.utf8_text(source).ok())
        }
        _ => None,
    };
    name == Some(var)
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
    fn flags_for_with_single_push() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(x); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_for_with_push_of_transform() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(x + 1); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_vec_macro_initializer() {
        let src = "fn f(src: Vec<u32>) { let mut dst = vec![]; for x in src { dst.push(x); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_with_capacity_initializer() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::with_capacity(src.len()); for x in src { dst.push(x); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_explicit_vec_type_annotation() {
        let src = "fn f(src: Vec<u32>) { let mut dst: Vec<u32> = make(); for x in src { dst.push(x); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_for_with_multiple_statements() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { let y = x + 1; dst.push(y); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_for_with_non_push_call() {
        let src = "fn f(src: Vec<u32>) { for x in src { println!(\"{}\", x); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_extend_call() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); dst.extend(src); }";
        assert!(run_on(src).is_empty());
    }

    // The receiver's `Vec`-ness can't be confirmed from a parameter binding,
    // so a push to an unknown receiver type is left alone.
    #[test]
    fn allows_push_to_unconfirmed_param() {
        let src = "fn f(src: Vec<u32>, mut dst: Vec<u32>) { for x in src { dst.push(x); } }";
        assert!(run_on(src).is_empty());
    }

    // crossbeam-deque Worker — push to a concurrent deque, not a Vec.
    #[test]
    fn allows_push_to_worker_deque_issue_1478() {
        let src = "fn f() { let w = Worker::new_fifo(); for i in 1..=3 { w.push(i); } }";
        assert!(run_on(src).is_empty());
    }

    // crossbeam-deque Injector — push to a concurrent queue, not a Vec.
    #[test]
    fn allows_push_to_injector_issue_1478() {
        let src = "fn f() { let q = Injector::new(); for i in 0..200 { q.push(i); } }";
        assert!(run_on(src).is_empty());
    }

    // crossbeam-epoch Queue with explicit type annotation — not a Vec.
    #[test]
    fn allows_push_to_typed_queue_issue_1478() {
        let src = "fn f() { let q: Queue<i64> = Queue::new(); for i in 0..200 { q.push(i) } }";
        assert!(run_on(src).is_empty());
    }

    // A push argument using `?` (fallible) can't be lifted into a `map`
    // closure, so `extend(...map(...))` would not compile.
    #[test]
    fn allows_fallible_push_argument_issue_3803() {
        let src = "fn build(src: &[u32]) -> Result<Vec<u32>, ()> { let mut dst = Vec::with_capacity(src.len()); for x in src { dst.push(transform(*x)?); } Ok(dst) }";
        assert!(run_on(src).is_empty());
    }

    // `?` nested deeper in the push argument is still fallible.
    #[test]
    fn allows_nested_fallible_push_argument_issue_3803() {
        let src = "fn build(src: &[u32]) -> Result<Vec<u32>, ()> { let mut dst = Vec::new(); for x in src { dst.push(transform(parse(x)?)); } Ok(dst) }";
        assert!(run_on(src).is_empty());
    }

    // The genuine target stays flagged: an infallible transform with no `?`.
    #[test]
    fn flags_infallible_transform_push_issue_3803() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(transform(x)); } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
