//! rust-match-lock-guard-scrutinee backend.
//!
//! Walk every `match_expression` and inspect its `value` (scrutinee).
//! If the scrutinee's outermost call is a no-argument `.lock()`, `.read()`,
//! or `.write()` — the standard `Mutex::lock` / `RwLock::read`/`write` guard
//! acquisition, all of which take no arguments — flag the match. The empty
//! argument list is what distinguishes a guard acquisition from `io::Read::read`
//! / `io::Write::write` and other custom `.read(buf)`/`.write(buf)` methods,
//! which take a buffer argument and are not lock guards. We also walk through
//! a single `?` (try_expression) so `match mtx.lock()?` is still caught.

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
        let inner = peel_guard_wrappers(value, source);
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

/// Peel the wrappers a guard acquisition is typically wrapped in so the
/// scrutinee's underlying `.lock()`/`.read()`/`.write()` is reached:
/// `?` (try), `.await`, and `.unwrap()` / `.expect(...)`. So `match
/// mtx.lock()?`, `match lock.read().unwrap()`, and `match m.lock().await`
/// all resolve to the lock call underneath.
fn peel_guard_wrappers<'a>(node: tree_sitter::Node<'a>, source: &[u8]) -> tree_sitter::Node<'a> {
    let mut current = node;
    loop {
        match current.kind() {
            "try_expression" | "await_expression" => {
                let Some(inner) = current.named_child(0) else {
                    return current;
                };
                current = inner;
            }
            "call_expression" => {
                let Some(receiver) = unwrap_or_expect_receiver(current, source) else {
                    return current;
                };
                current = receiver;
            }
            _ => return current,
        }
    }
}

/// If `node` is `<receiver>.unwrap()` or `<receiver>.expect(..)`, return the
/// receiver expression. These are the idiomatic ways to discard the
/// `LockResult`/`PoisonError` wrapper around a guard.
fn unwrap_or_expect_receiver<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let function = node.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }
    let field = function.child_by_field_name("field")?;
    let name = field.utf8_text(source).ok()?;
    if name != "unwrap" && name != "expect" {
        return None;
    }
    function.child_by_field_name("value")
}

/// If `node` is a no-argument `expr.method()`, return `method`. Used to
/// identify guard acquisitions `.lock()` / `.read()` / `.write()` at the top
/// of an expression. The empty argument list is required: `Mutex::lock` and
/// `RwLock::read`/`write` take no arguments, whereas `io::Read::read(&mut buf)`,
/// `io::Write::write(&data)`, and custom `.read(buf)`/`.write(buf)` methods
/// pass a buffer — those are not lock guards and must not match.
fn outermost_method_call(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    if node.kind() != "call_expression" {
        return None;
    }
    let function = node.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }
    let arguments = node.child_by_field_name("arguments")?;
    if arguments.named_child_count() != 0 {
        return None;
    }
    let field = function.child_by_field_name("field")?;
    field.utf8_text(source).ok().map(|s| s.to_string())
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

    #[test]
    fn allows_io_read_with_buffer_arg() {
        let src = "fn f() { match reader.read(&mut buf) { Ok(0) => (), _ => () } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_io_write_with_buffer_arg() {
        let src = "fn f() { match w.write(&data) { Ok(_) => (), Err(_) => () } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_rwlock_read_guard() {
        let src = "fn f() { match lock.read().unwrap() { _ => () } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_mutex_lock_guard() {
        let src = "fn f() { match m.lock().unwrap() { _ => () } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
