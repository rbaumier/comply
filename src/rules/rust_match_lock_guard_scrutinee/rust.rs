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
//!
//! `Mutex::lock` / `RwLock::read`/`write` return a `LockResult<Guard>`. When the
//! arms destructure that `LockResult` (every arm is `Ok(..)` / `Err(..)`, with an
//! optional catch-all alongside) the match operates on the result, not the
//! guarded value: the `Err` arm holds no lock and the `Ok` arm binds the guard
//! into the surrounding scope. That is the idiomatic poison-handling shape, so it
//! is not flagged. The anti-pattern this rule targets is value-matching the guard
//! itself (`match m.lock().unwrap() { Variant => … }`), where each value-arm runs
//! with the lock held.

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
        if let Some(match_block) = node.child_by_field_name("body")
            && arms_destructure_lock_result(match_block, source)
        {
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
/// `?` (try) and `.unwrap()` / `.expect(...)`. So `match mtx.lock()?` and
/// `match lock.read().unwrap()` resolve to the lock call underneath.
///
/// `.await` is deliberately not peeled: `std::sync` `Mutex`/`RwLock` guards
/// (the only locks held across every match arm, and all this rule targets) are
/// acquired synchronously and never awaited, so an awaited `.read()`/`.write()`/
/// `.lock()` is an ordinary async method returning `Result`/`Option`, not a lock
/// guard, and must not be treated as one.
fn peel_guard_wrappers<'a>(node: tree_sitter::Node<'a>, source: &[u8]) -> tree_sitter::Node<'a> {
    let mut current = node;
    loop {
        match current.kind() {
            "try_expression" => {
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

/// True if the `match_block`'s arms destructure a `LockResult` rather than
/// value-match the guard: at least one arm pattern is a `Result` constructor
/// (`Ok(..)` / `Err(..)`) and every other arm is either a `Result` constructor
/// or a catch-all (`_` or a bare binding). In that shape the scrutinee is the
/// raw `LockResult`, the lock is correctly scoped (the `Ok` arm binds the guard
/// outward, the `Err` arm holds nothing), and the match must not be flagged.
fn arms_destructure_lock_result(match_block: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = match_block.walk();
    let mut saw_result_ctor = false;
    for arm in match_block.named_children(&mut cursor) {
        if arm.kind() != "match_arm" {
            continue;
        }
        let Some(match_pattern) = arm.child_by_field_name("pattern") else {
            return false;
        };
        match classify_arm(match_pattern, source) {
            ArmShape::ResultCtor => saw_result_ctor = true,
            ArmShape::CatchAll => {}
            ArmShape::Other => return false,
        }
    }
    saw_result_ctor
}

enum ArmShape {
    /// `Ok(..)` / `Err(..)` (or an or-pattern of only those) — destructures the
    /// `LockResult`.
    ResultCtor,
    /// `_` or a bare binding — matches the whole scrutinee, lock stays scoped.
    CatchAll,
    /// Anything else — the arm value-matches the guarded data.
    Other,
}

/// Classify an arm's pattern. An `or_pattern` is a `ResultCtor` only when every
/// branch is itself a `Result` constructor. The `match_pattern` wrapper (with an
/// optional `if`-guard trailing the pattern) is unwrapped to its first named
/// child; a guard never changes whether the pattern destructures the result.
fn classify_arm(pattern: tree_sitter::Node, source: &[u8]) -> ArmShape {
    match pattern.kind() {
        // `_` surfaces as an unnamed token, so the wrapper has no named child.
        "match_pattern" => match pattern.named_child(0) {
            Some(inner) => classify_arm(inner, source),
            None if matches!(pattern.utf8_text(source), Ok("_")) => ArmShape::CatchAll,
            None => ArmShape::Other,
        },
        "tuple_struct_pattern" if pattern_is_result_ctor(pattern, source) => ArmShape::ResultCtor,
        "wildcard_pattern" | "identifier" => ArmShape::CatchAll,
        "or_pattern" => {
            let mut cursor = pattern.walk();
            let all_result_ctors = pattern
                .named_children(&mut cursor)
                .all(|branch| matches!(classify_arm(branch, source), ArmShape::ResultCtor));
            if all_result_ctors {
                ArmShape::ResultCtor
            } else {
                ArmShape::Other
            }
        }
        _ => ArmShape::Other,
    }
}

/// True if `pattern` is an `Ok(..)` / `Err(..)` tuple-struct pattern, the two
/// `Result` constructors a `LockResult` is destructured into. The constructor
/// may be path-qualified (`Result::Ok(..)`); the final segment is what counts.
fn pattern_is_result_ctor(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(type_node) = pattern.child_by_field_name("type") else {
        return false;
    };
    let name = match type_node.kind() {
        "identifier" => type_node.utf8_text(source).ok(),
        "scoped_identifier" => type_node
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(source).ok()),
        _ => None,
    };
    matches!(name, Some("Ok" | "Err"))
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
    fn allows_match_destructuring_lock_result() {
        let src = "fn f() { match m.lock() { Ok(g) => (), Err(_) => () } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_match_destructuring_read_result() {
        let src = "fn f() { match rw.read() { Ok(g) => (), Err(_) => () } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_match_destructuring_write_result() {
        let src = "fn f() { match rw.write() { Ok(g) => (), Err(_) => () } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_match_destructuring_lock_result_with_early_return() {
        // The nushell `nu-lsp` shape: destructure the `LockResult`, bind the
        // guard in the `Ok` arm, return early on poison.
        let src = "fn f() -> Result<u32, String> { \
            let docs = match self.docs.lock() { \
                Ok(it) => it, \
                Err(err) => return Err(err.to_string()), \
            }; \
            Ok(docs.len() as u32) \
        }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_match_destructuring_with_catch_all_arm() {
        let src = "fn f() { match m.lock() { Ok(g) => (), _ => () } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_match_destructuring_or_pattern() {
        let src = "fn f() { match m.lock() { Ok(a) | Err(a) => () } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_match_on_lock_with_try() {
        let src = "fn f() -> Result<(), ()> { match m.lock()? { _ => Ok(()) } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_value_match_on_unwrapped_guard() {
        let src = "fn f() { match m.lock().unwrap() { Foo::A => 1, Foo::B => 2 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_value_match_on_expected_guard() {
        let src = "fn f() { match m.lock().expect(\"x\") { Foo::A => 1, _ => 2 } }";
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
    fn allows_awaited_write_with_result_arms() {
        // Issue #3749 repro: an awaited async `write` returning a `Result`, not a
        // lock guard. `.await` is never peeled, so the underlying `.write()` is
        // not reached and the match is not treated as a guard acquisition.
        let src = "async fn run(bundle: &mut Bundler) { let o = match bundle.write().await { Ok(output) => output, Err(_) => return }; let _ = o; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_value_match_after_awaited_write() {
        // The genuine await-peel FP: value-matching an owned enum returned by an
        // awaited async method named `write`. A synchronous lock guard is never
        // awaited, so this is an async method, not a lock acquisition.
        let src = "async fn run(s: &mut Stream) { match s.write().await { WriteOutcome::Done => {}, WriteOutcome::Partial(n) => {} } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_value_match_after_awaited_read() {
        let src = "async fn f(m: &mut W) { match m.read().await { Foo::A => 1, Foo::B => 2 }; }";
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

    #[test]
    fn flags_value_match_on_sync_lock_guard() {
        let src = "fn f() { match m.lock().unwrap() { Foo::A => 1, _ => 2 } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
