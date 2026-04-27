//! rust-if-let-mutex-lock backend.
//!
//! Walks `if_let_expression` nodes whose scrutinee is a method call
//! ending in `.lock()` / `.read()` / `.write()` / `.try_lock()` /
//! `.try_read()` / `.try_write()`. The lock guard built by the
//! scrutinee is alive for the entire `if/else`, so the `else`
//! branch silently still holds the lock — usually the opposite of
//! what the author intended.
//!
//! Tree-sitter Rust represents `if let` as an `if_expression` whose
//! `condition` field holds a `let_condition` (or, in some grammar
//! versions, `let_chain`). We accept both shapes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["if_expression"];

const LOCK_METHODS: &[&str] = &[
    "lock",
    "read",
    "write",
    "try_lock",
    "try_read",
    "try_write",
];

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
        // Only flag `if let` (must have an `else` branch + a let condition).
        let Some(condition) = node.child_by_field_name("condition") else {
            return;
        };
        // Find the actual scrutinee inside a let_condition / let_chain.
        let Some(scrutinee) = find_let_scrutinee(condition) else {
            return;
        };
        if !is_lock_call(scrutinee, source_bytes) {
            return;
        }
        // Only flag when there's an `else` branch — otherwise the
        // scope ends with the `if`, which is fine.
        if node.child_by_field_name("alternative").is_none() {
            return;
        }
        if let Some(method) = lock_method_name(scrutinee, source_bytes) {
            diagnostics.push(Diagnostic::at_node(
                std::sync::Arc::clone(&ctx.path_arc),
                &node,
                "rust-if-let-mutex-lock",
                format!(
                    "`if let ... = .{method}()` keeps the guard alive across \
                     the `else` branch. Bind the guard in a separate `let` \
                     before the `if let`, or restructure with `match` + `drop`."
                ),
                Severity::Error,
            ));
        }
    }
}

fn find_let_scrutinee(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    // Walk into let_condition / let_chain looking for a `value`/`scrutinee`
    // field that's a call_expression.
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "let_condition"
            && let Some(v) = n
                .child_by_field_name("value")
                .or_else(|| n.child_by_field_name("scrutinee"))
        {
            return Some(v);
        }
        for c in n.children(&mut cursor) {
            stack.push(c);
        }
    }
    None
}

fn lock_method_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "call_expression" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    if func.kind() != "field_expression" {
        return None;
    }
    let field = func.child_by_field_name("field")?;
    let name = field.utf8_text(source).ok()?;
    if LOCK_METHODS.contains(&name) {
        Some(name)
    } else {
        None
    }
}

fn is_lock_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    lock_method_name(node, source).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_if_let_mutex_lock_with_else() {
        let source = "fn f() { if let Ok(g) = m.lock() { go(g); } else { fallback(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_if_let_rwlock_read_with_else() {
        let source = "fn f() { if let Ok(g) = rw.read() { go(g); } else { fallback(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_if_let_lock_without_else() {
        let source = "fn f() { if let Ok(g) = m.lock() { go(g); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_match_on_lock() {
        let source = "fn f() { match m.lock() { Ok(g) => go(g), _ => fallback(), } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_if_let_unrelated_call() {
        let source = "fn f() { if let Some(x) = compute() { use_(x); } else { default(); } }";
        assert!(run_on(source).is_empty());
    }
}
