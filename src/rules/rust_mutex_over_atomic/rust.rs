//! rust-mutex-over-atomic backend.
//!
//! Walks `generic_type` nodes whose base name is `Mutex` or `RwLock` — the last
//! `::` segment, so `std::sync::Mutex`, `tokio::sync::RwLock` and
//! `parking_lot::Mutex` all match — and whose sole type argument is a
//! `primitive_type` that has an `AtomicX` counterpart in
//! `std::sync::atomic`.
//!
//! Restricting the payload to a single `primitive_type` node is what keeps the
//! composite cases out on their own: `Mutex<Option<bool>>` and `Mutex<Vec<u8>>`
//! parse as `generic_type`, `Mutex<(bool, u32)>` as `tuple_type`. A float is a
//! `primitive_type` but absent from the atomic table — std has no `AtomicF64` —
//! and so is `u128`/`i128`.
//!
//! No ancestor is exempt: the lock is just as replaceable as a struct field, a
//! local, or the inner half of an `Arc<Mutex<bool>>`.
//!
//! Three escapes:
//!
//! - an atomic polyfill: a struct whose name starts with `Atomic` (tokio's
//!   `AtomicU64 { inner: Mutex<u64> }` for targets without 64-bit atomics), or
//!   a file that mentions `target_has_atomic` — the code is explicitly the
//!   fallback for when the atomic the rule would suggest does not exist.
//! - a file mentioning `Condvar` anywhere. There the mutex is the condvar's
//!   companion — `Condvar::wait` takes the guard and needs a real lock to
//!   release and reacquire — so the payload's shape says nothing about whether
//!   the lock is warranted. Exempting the whole file rather than the single
//!   declaration is deliberate: the mutex and the condvar that pairs with it are
//!   routinely fields of different structs.
//! - test code, where a lock around a counter is scaffolding, not a hot path.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;

/// Scalars that have a drop-in atomic in `std::sync::atomic`. `u128`, `i128`,
/// `f32`, `f64` and `char` are absent on purpose: no atomic exists for them, so
/// the lock is the only option.
const ATOMIC_FOR_PRIMITIVE: &[(&str, &str)] = &[
    ("bool", "AtomicBool"),
    ("usize", "AtomicUsize"),
    ("isize", "AtomicIsize"),
    ("u8", "AtomicU8"),
    ("u16", "AtomicU16"),
    ("u32", "AtomicU32"),
    ("u64", "AtomicU64"),
    ("i8", "AtomicI8"),
    ("i16", "AtomicI16"),
    ("i32", "AtomicI32"),
    ("i64", "AtomicI64"),
];

crate::ast_check! { on ["generic_type"] prefilter = ["Mutex<", "RwLock<"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }

    let Some(type_node) = node.child_by_field_name("type") else { return; };
    let type_text = type_node.utf8_text(source).unwrap_or("");
    let base = type_text.rsplit("::").next().unwrap_or("");
    if base != "Mutex" && base != "RwLock" { return; }

    let Some(primitive) = sole_primitive_argument(node, source) else { return; };
    let Some(atomic) = atomic_counterpart(primitive) else { return; };

    if file_uses_condvar(source) { return; }
    if is_atomic_polyfill(node, source) { return; }
    if is_in_test_context(node, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{base}<{primitive}>` blocks a thread to touch a single scalar. \
             Use `{atomic}` — same reads, writes and read-modify-writes, lock-free and without a guard to hold."
        ),
        Severity::Error,
    ));
}

/// The scalar a lock wraps, when it wraps exactly one and that one is a bare
/// primitive. Anything composite — `Option<bool>`, `(bool, u32)`, `Vec<u8>` —
/// parses as another node kind and yields `None`, because only a lock can give
/// it the all-or-nothing update an atomic cannot.
fn sole_primitive_argument<'a>(generic: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let arguments = generic.child_by_field_name("type_arguments")?;
    if arguments.named_child_count() != 1 {
        return None;
    }
    let argument = arguments.named_child(0)?;
    if argument.kind() != "primitive_type" {
        return None;
    }
    argument.utf8_text(source).ok()
}

fn atomic_counterpart(primitive: &str) -> Option<&'static str> {
    ATOMIC_FOR_PRIMITIVE
        .iter()
        .find(|(name, _)| *name == primitive)
        .map(|(_, atomic)| *atomic)
}

/// The lock IS the atomic on this target: a `struct Atomic…` built on a mutex,
/// or a file gated on `target_has_atomic`, exists precisely because the atomic
/// the diagnostic would name is unavailable there.
fn is_atomic_polyfill(node: tree_sitter::Node, source: &[u8]) -> bool {
    if std::str::from_utf8(source).is_ok_and(|text| text.contains("target_has_atomic")) {
        return true;
    }
    let mut cur = node.parent();
    while let Some(parent) = cur {
        if parent.kind() == "struct_item" {
            return parent
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .is_some_and(|name| name.starts_with("Atomic"));
        }
        cur = parent.parent();
    }
    false
}

/// A `Condvar` anywhere in the file means some mutex is there to be handed to
/// `Condvar::wait`, which needs a lock it can release and reacquire. Which
/// mutex that is cannot be read off the declaration — the condvar is commonly a
/// sibling field, or lives in another type entirely — so the whole file steps
/// aside.
fn file_uses_condvar(source: &[u8]) -> bool {
    std::str::from_utf8(source).is_ok_and(|text| text.contains("Condvar"))
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

    fn message(src: &str) -> String {
        let diagnostics = run(src);
        assert_eq!(diagnostics.len(), 1, "expected exactly one diagnostic");
        diagnostics[0].message.clone()
    }

    #[test]
    fn flags_mutex_bool_field() {
        assert_eq!(run("struct S { ready: Mutex<bool> }").len(), 1);
    }

    #[test]
    fn flags_mutex_usize_local() {
        assert_eq!(
            run("fn f() { let c: Mutex<usize> = Mutex::new(0); }").len(),
            1
        );
    }

    #[test]
    fn flags_arc_mutex_bool() {
        assert_eq!(run("struct S { ready: Arc<Mutex<bool>> }").len(), 1);
    }

    #[test]
    fn flags_qualified_mutex_variants() {
        assert_eq!(run("struct S { n: std::sync::Mutex<u64> }").len(), 1);
        assert_eq!(run("struct S { n: tokio::sync::Mutex<i32> }").len(), 1);
        assert_eq!(run("struct S { n: parking_lot::Mutex<isize> }").len(), 1);
    }

    #[test]
    fn flags_rwlock_over_primitive() {
        assert_eq!(run("struct S { n: RwLock<u32> }").len(), 1);
        assert_eq!(run("struct S { on: parking_lot::RwLock<bool> }").len(), 1);
    }

    #[test]
    fn message_names_the_matching_atomic() {
        assert!(message("struct S { ready: Mutex<bool> }").contains("`AtomicBool`"));
        assert!(message("struct S { n: RwLock<usize> }").contains("`AtomicUsize`"));
        assert!(message("struct S { n: Mutex<u32> }").contains("`AtomicU32`"));
    }

    #[test]
    fn allows_atomic_polyfill_struct() {
        assert!(run("pub struct AtomicU64 { inner: Mutex<u64> }").is_empty());
    }

    #[test]
    fn allows_file_gated_on_target_has_atomic() {
        let src = "#[cfg(not(target_has_atomic = \"64\"))]\nstruct Counter { inner: Mutex<u64> }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_over_option() {
        assert!(run("struct S { ready: Mutex<Option<bool>> }").is_empty());
    }

    #[test]
    fn allows_mutex_over_tuple() {
        assert!(run("struct S { state: Mutex<(bool, u32)> }").is_empty());
    }

    #[test]
    fn allows_mutex_over_collection() {
        assert!(run("struct S { buf: Mutex<Vec<u8>> }").is_empty());
    }

    #[test]
    fn allows_mutex_over_float() {
        assert!(run("struct S { ratio: Mutex<f64> }").is_empty());
        assert!(run("struct S { ratio: Mutex<f32> }").is_empty());
    }

    #[test]
    fn allows_mutex_over_int_without_atomic() {
        assert!(run("struct S { n: Mutex<u128> }").is_empty());
    }

    #[test]
    fn allows_mutex_bool_in_file_using_condvar() {
        let src = r#"
struct Gate { ready: Mutex<bool>, signal: Condvar }
fn wait(gate: &Gate) {
    let mut ready = gate.ready.lock().unwrap();
    while !*ready { ready = gate.signal.wait(ready).unwrap(); }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_bool_in_test_module() {
        assert!(run("#[cfg(test)]\nmod tests { struct S { on: Mutex<bool> } }").is_empty());
    }

    #[test]
    fn allows_unrelated_generic_over_primitive() {
        assert!(run("struct S { n: Cell<u32> }").is_empty());
        assert!(run("struct S { n: RefCell<bool> }").is_empty());
    }
}
