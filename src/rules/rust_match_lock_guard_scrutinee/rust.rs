//! rust-match-lock-guard-scrutinee backend.
//!
//! Walk every `match_expression` and inspect its `value` (scrutinee).
//! If the scrutinee's outermost call is `.lock()`, `.read()`, or `.write()`
//! — the standard `Mutex` / `RwLock` API — flag the match. We also walk
//! through a single `?` (try_expression) so `match mtx.lock()?` is still
//! caught.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["match_expression"];

const LOCK_METHODS: &[&str] = &["lock", "read", "write"];

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
        let Some(value) = node.child_by_field_name("value") else {
            return;
        };
        let inner = unwrap_try(value);
        let Some(method) = outermost_method_call(inner, source) else {
            return;
        };
        if !LOCK_METHODS.contains(&method.as_str()) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-match-lock-guard-scrutinee",
            format!(
                "`match <expr>.{method}() {{ … }}` keeps the lock held \
                 through every arm. Bind the locked value first \
                 (`let guard = …; let v = guard.clone(); drop(guard);`) \
                 then match on `v`."
            ),
            Severity::Error,
        ));
    }
}

fn unwrap_try(node: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    if node.kind() == "try_expression"
        && let Some(inner) = node.named_child(0)
    {
        return inner;
    }
    node
}

/// If `node` is `expr.method()`, return `method`. Used to identify
/// `.lock()` / `.read()` / `.write()` at the top of an expression.
fn outermost_method_call(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    if node.kind() != "call_expression" {
        return None;
    }
    let function = node.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }
    let field = function.child_by_field_name("field")?;
    field.utf8_text(source).ok().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_match_on_lock() {
        let src = "fn f() { match m.lock() { Ok(g) => (), Err(_) => () } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_match_on_read() {
        let src = "fn f() { match rw.read() { Ok(g) => (), Err(_) => () } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_match_on_write() {
        let src = "fn f() { match rw.write() { Ok(g) => (), Err(_) => () } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_match_on_lock_with_try() {
        let src = "fn f() -> Result<(), ()> { match m.lock()? { _ => Ok(()) } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_match_on_plain_expr() {
        let src = "fn f(x: u32) { match x { 0 => (), _ => () } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_match_on_unrelated_method() {
        let src = "fn f() { match v.first() { Some(_) => (), None => () } }";
        assert!(run_on(src).is_empty());
    }
}
