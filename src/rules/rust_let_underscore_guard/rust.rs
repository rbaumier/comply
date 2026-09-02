//! rust-let-underscore-guard backend.
//!
//! Walks `let_declaration` nodes whose pattern is exactly `_` and whose value
//! produces an RAII guard. The value is followed down through the wrappers that
//! only hand the guard along — `?`, `.await`, `.unwrap()`, `.expect(..)` — so
//! `let _ = m.lock().unwrap();` and `let _ = m.lock().await;` are reached as
//! readily as `let _ = m.lock();`.
//!
//! A guard is recognised in two shapes:
//!
//! - a **zero-argument** method call named `lock`, `try_lock`, `read`, `write`,
//!   `enter` or `entered`. The arity requirement is what separates the guard
//!   methods from their same-named `std::io` twins: `RwLock::read()` takes no
//!   argument, `Read::read(&mut buf)` does, and discarding the byte count of an
//!   I/O call is a different (and legitimate) pattern. `read` and `write` carry
//!   one more condition — the file must name `RwLock` — because they are the
//!   only two whose bare name is claimed by unrelated APIs that return a plain
//!   value, a memory-mapped register read being the usual one.
//! - a temp-path constructor: `tempdir` under any path, or `TempDir::new` /
//!   `NamedTempFile::new` matched on the last two path segments, since a bare
//!   `new` says nothing about what it builds.
//!
//! Everything else stays silent, which covers the common deliberate discards:
//! `let _ = tx.send(v);`, `let _ = fs::remove_file(p);`,
//! `let _ = std::mem::replace(&mut a, b);`. So does a lock in argument position
//! (`let _ = f(m.lock());`) — only the outermost call of the discarded
//! expression is inspected, and there it is `f`, not `lock`.
//!
//! Test code is exempt: a lock taken and dropped on the spot in a test is
//! usually just asserting the lock is free.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_test_context, rust_path_segments};

/// Zero-argument methods that return an RAII guard rather than a plain value.
const GUARD_METHODS: &[&str] = &["lock", "try_lock", "read", "write", "enter", "entered"];

/// The two [`GUARD_METHODS`] whose name alone is not evidence: only `RwLock`
/// spells its guard accessors `read()` / `write()`, so outside a file that
/// mentions one they are some other API returning a plain value.
const RWLOCK_ONLY_METHODS: &[&str] = &["read", "write"];

/// Guard constructors that must be qualified by their type to count: matched on
/// the last two path segments, so `tempfile::TempDir::new` hits and any other
/// `::new` does not.
const GUARD_CONSTRUCTOR_PATHS: &[&str] = &["TempDir::new", "NamedTempFile::new"];

/// Guard constructors whose own name is specific enough, matched on the last
/// path segment alone (`tempdir()`, `tempfile::tempdir()`).
const GUARD_CONSTRUCTOR_FNS: &[&str] = &["tempdir"];

/// Methods that pull the guard out of the `Result` it arrived in without
/// changing what the value is — the search continues through them.
const RESULT_UNWRAPPERS: &[&str] = &["unwrap", "expect"];

/// What the guard was holding open, used to spell out the consequence of
/// dropping it early.
#[derive(Clone, Copy)]
enum GuardKind {
    Lock,
    Span,
    TempPath,
}

impl GuardKind {
    fn consequence(self) -> &'static str {
        match self {
            GuardKind::Lock => "the lock is released again before the next statement",
            GuardKind::Span => "the span closes again before the next statement",
            GuardKind::TempPath => {
                "the temporary file or directory is deleted again before the next statement"
            }
        }
    }
}

crate::ast_check! { on ["let_declaration"] prefilter = ["let _"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }

    // `_` is the only pattern that drops at the end of the statement. `let _x`
    // and `let _guard` are named bindings that live to the end of the scope —
    // and are the fix this rule asks for.
    let Some(pattern) = node.child_by_field_name("pattern") else { return; };
    if pattern.utf8_text(source) != Ok("_") { return; }

    let Some(value) = node.child_by_field_name("value") else { return; };
    let Some((kind, call)) = guard_producer(value, source) else { return; };

    if is_in_test_context(node, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`let _ = …` binds nothing, so the guard from `{call}` is dropped right here and {}. \
             Bind it instead: `let _guard = …`.",
            kind.consequence()
        ),
        Severity::Error,
    ));
}

/// The guard-producing call inside a discarded expression, if there is one.
/// Descends through the wrappers that only carry the guard along, so the search
/// ends on the call that actually built it.
fn guard_producer(expr: tree_sitter::Node, source: &[u8]) -> Option<(GuardKind, String)> {
    let mut current = expr;
    loop {
        match current.kind() {
            // `let _ = m.lock()?;` / `let _ = m.lock().await;`
            "try_expression" | "await_expression" => current = current.named_child(0)?,
            "call_expression" => {
                let func = current.child_by_field_name("function")?;
                if func.kind() != "field_expression" {
                    return constructor_guard(func, source);
                }
                let method = func.child_by_field_name("field")?.utf8_text(source).ok()?;
                if RESULT_UNWRAPPERS.contains(&method) {
                    current = func.child_by_field_name("value")?;
                    continue;
                }
                if GUARD_METHODS.contains(&method)
                    && call_takes_no_arguments(current)
                    && (!RWLOCK_ONLY_METHODS.contains(&method) || file_uses_rwlock(source))
                {
                    return Some((method_guard_kind(method), method.to_string()));
                }
                return None;
            }
            _ => return None,
        }
    }
}

/// The guard kind of a recognised call whose function is a plain path rather
/// than a method — the temp-path constructors.
fn constructor_guard(func: tree_sitter::Node, source: &[u8]) -> Option<(GuardKind, String)> {
    let segments = rust_path_segments(func, source);
    let (last, leading) = segments.split_last()?;
    if GUARD_CONSTRUCTOR_FNS.contains(&last.as_str()) {
        return Some((GuardKind::TempPath, last.clone()));
    }
    let qualified = format!("{}::{last}", leading.last()?);
    GUARD_CONSTRUCTOR_PATHS
        .contains(&qualified.as_str())
        .then_some((GuardKind::TempPath, qualified))
}

fn method_guard_kind(method: &str) -> GuardKind {
    match method {
        "enter" | "entered" => GuardKind::Span,
        _ => GuardKind::Lock,
    }
}

/// True when the call's argument list is empty. The guard methods all take no
/// argument; their `std::io` namesakes (`read`, `write`) take a buffer, so
/// arity alone tells the two families apart.
fn call_takes_no_arguments(call: tree_sitter::Node) -> bool {
    call.child_by_field_name("arguments")
        .is_some_and(|arguments| arguments.named_child_count() == 0)
}

/// True when the file names `RwLock` — std, tokio and parking_lot all spell it
/// the same way, and it is the one type whose `read()` / `write()` hand back a
/// guard.
fn file_uses_rwlock(source: &[u8]) -> bool {
    std::str::from_utf8(source).is_ok_and(|text| text.contains("RwLock"))
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_discarded_mutex_lock() {
        assert_eq!(run("fn f(m: &Mutex<u32>) { let _ = m.lock(); }").len(), 1);
    }

    #[test]
    fn flags_discarded_lock_unwrap() {
        assert_eq!(
            run("fn f(m: &Mutex<u32>) { let _ = m.lock().unwrap(); }").len(),
            1
        );
    }

    #[test]
    fn flags_discarded_lock_expect() {
        assert_eq!(
            run("fn f(m: &Mutex<u32>) { let _ = m.lock().expect(\"poisoned\"); }").len(),
            1
        );
    }

    #[test]
    fn flags_discarded_lock_with_question_mark() {
        let src = "fn f(m: &Mutex<u32>) -> Result<(), E> { let _ = m.lock()?; Ok(()) }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_discarded_async_lock() {
        let src = "async fn f(m: &tokio::sync::Mutex<u32>) { let _ = m.lock().await; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_discarded_try_lock() {
        assert_eq!(run("fn f(m: &Mutex<u32>) { let _ = m.try_lock(); }").len(), 1);
    }

    #[test]
    fn flags_discarded_rwlock_read_and_write() {
        assert_eq!(run("fn f(l: &RwLock<u32>) { let _ = l.read(); }").len(), 1);
        assert_eq!(run("fn f(l: &RwLock<u32>) { let _ = l.write(); }").len(), 1);
    }

    #[test]
    fn flags_discarded_span_enter() {
        assert_eq!(run("fn f(span: &Span) { let _ = span.enter(); }").len(), 1);
    }

    #[test]
    fn flags_discarded_span_entered() {
        let src = r#"fn f() { let _ = tracing::span!(Level::INFO, "work").entered(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_discarded_temp_dir() {
        assert_eq!(run("fn f() { let _ = TempDir::new().unwrap(); }").len(), 1);
        assert_eq!(run("fn f() { let _ = tempfile::tempdir().unwrap(); }").len(), 1);
        assert_eq!(
            run("fn f() { let _ = tempfile::NamedTempFile::new().unwrap(); }").len(),
            1
        );
    }

    #[test]
    fn allows_discarded_channel_send() {
        assert!(run("fn f(tx: &Sender<u32>) { let _ = tx.send(1); }").is_empty());
    }

    #[test]
    fn allows_discarded_fs_call() {
        assert!(run("fn f(p: &Path) { let _ = std::fs::remove_file(p); }").is_empty());
    }

    #[test]
    fn allows_discarded_mem_replace() {
        assert!(run("fn f(a: &mut u32) { let _ = std::mem::replace(a, 0); }").is_empty());
    }

    #[test]
    fn allows_named_guard_binding() {
        assert!(run("fn f(m: &Mutex<u32>) { let _guard = m.lock().unwrap(); }").is_empty());
        assert!(run("fn f(m: &Mutex<u32>) { let _g = m.lock(); }").is_empty());
        assert!(run("fn f(m: &Mutex<u32>) { let guard = m.lock(); }").is_empty());
    }

    #[test]
    fn allows_lock_in_argument_position() {
        assert!(run("fn f(m: &Mutex<u32>) { let _ = g(m.lock()); }").is_empty());
    }

    #[test]
    fn allows_io_read_and_write_with_buffer() {
        assert!(run("fn f(s: &mut TcpStream, b: &mut [u8]) { let _ = s.read(b); }").is_empty());
        assert!(run("fn f(s: &mut TcpStream, b: &[u8]) { let _ = s.write(b); }").is_empty());
    }

    #[test]
    fn allows_zero_arg_read_outside_an_rwlock_file() {
        assert!(run("fn f(reg: &StatusRegister) { let _ = reg.read(); }").is_empty());
    }

    #[test]
    fn allows_value_derived_from_a_guard() {
        assert!(run("fn f(m: &Mutex<u32>) { let _ = m.lock().unwrap().clone(); }").is_empty());
    }

    #[test]
    fn allows_unrelated_new_constructor() {
        assert!(run("fn f() { let _ = String::new(); }").is_empty());
    }

    #[test]
    fn allows_lock_discarded_in_test_module() {
        let src = "#[cfg(test)]\nmod tests { fn t(m: &Mutex<u32>) { let _ = m.lock(); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_lock_discarded_in_test_fn() {
        let src = "#[test]\nfn t() { let m = Mutex::new(0); let _ = m.lock(); }";
        assert!(run(src).is_empty());
    }
}
