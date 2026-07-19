use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    is_async_mutex_type, is_in_test_context, is_suppressed_by_clippy_allow, is_under_tests_dir,
};

const ATOMIC_TYPES: &[(&str, &str)] = &[
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

crate::ast_check! { on ["type_identifier"] prefilter = ["Mutex"] => |node, source, ctx, diagnostics|
    // Test fixtures deliberately exercise `Mutex<primitive>` shared-state
    // plumbing (e.g. `Arc<Mutex<u32>>` passed through a handler); the
    // lock-free-atomic discipline only applies to production code. Both an
    // in-file `#[cfg(test)]` module and a standalone integration-test support
    // module under `tests/` (which carries no `#[cfg(test)]` attribute) are
    // test-only and exempt.
    if is_in_test_context(node, source) || is_under_tests_dir(ctx.path) {
        return;
    }

    let Ok(text) = node.utf8_text(source) else { return };
    if text != "Mutex" { return; }

    // Locate the `generic_type` wrapping this `Mutex`. An unqualified `Mutex<…>`
    // is the direct parent; a qualified `tokio::sync::Mutex<…>` nests the name
    // under a `scoped_type_identifier` whose parent is the `generic_type`.
    let Some(parent) = node.parent() else { return };
    let generic = match parent.kind() {
        "generic_type" => parent,
        "scoped_type_identifier" => {
            let Some(grandparent) = parent.parent() else { return };
            if grandparent.kind() != "generic_type" { return; }
            if grandparent.child_by_field_name("type") != Some(parent) { return; }
            grandparent
        }
        _ => return,
    };

    // Async mutexes (`tokio` / `futures` / `async_std` / `async_lock`) exist to
    // hold a lock across `.await` points; `AtomicX` has no `.lock().await` and
    // cannot serialize an async critical section, so it is not a valid
    // replacement. Resolve which `Mutex` this is via import provenance and skip
    // the async ones — only `std::sync::Mutex` / `parking_lot::Mutex` have an
    // atomic drop-in equivalent.
    if is_async_mutex_type(node, source) {
        return;
    }

    // A `Mutex` paired with a `Condvar` sibling field is the condition-variable
    // latch pattern: `Condvar::wait` takes a `MutexGuard`, so the `Mutex` is
    // structurally required and has no atomic equivalent. Skip it.
    if has_condvar_sibling_field(generic, source) {
        return;
    }

    // `#[allow(clippy::mutex_atomic)]` / `#[expect(...)]` on the field or its
    // enclosing struct is the author explicitly overriding the equivalent
    // clippy lint (e.g. the value is guarded together with sibling fields under
    // one lock). Honor that suppression like the other clippy-mirroring rules.
    if is_suppressed_by_clippy_allow(node, &["mutex_atomic"], source) {
        return;
    }

    // The single type argument must be a primitive with an atomic counterpart.
    let Some(type_args) = generic.child_by_field_name("type_arguments") else { return };
    let mut cursor = type_args.walk();
    let mut args = type_args.named_children(&mut cursor);
    let Some(arg) = args.next() else { return };
    if args.next().is_some() { return; }
    let Ok(arg_text) = arg.utf8_text(source) else { return };

    for &(prim, atomic) in ATOMIC_TYPES {
        if arg_text == prim {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &generic,
                super::META.id,
                format!("`Mutex<{prim}>` — prefer `{atomic}` for lock-free access."),
                Severity::Error,
            ));
            return;
        }
    }
}

/// True if the `Mutex` at `mutex_type` is a field of a struct that also declares
/// a sibling field whose type contains `Condvar` (e.g. `Condvar`,
/// `std::sync::Condvar`, `Arc<Condvar>`). Such a pair is a condition-variable
/// latch where the `Mutex` is required and cannot be replaced by an atomic.
fn has_condvar_sibling_field(mutex_type: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = mutex_type;
    while let Some(parent) = current.parent() {
        if parent.kind() == "field_declaration_list" {
            return field_list_has_condvar(parent, source);
        }
        current = parent;
    }
    false
}

fn field_list_has_condvar(list: tree_sitter::Node, source: &[u8]) -> bool {
    let field_count = list.named_child_count();
    for i in 0..field_count {
        if let Some(field) = list.named_child(i)
            && field.kind() == "field_declaration"
            && let Some(ty) = field.child_by_field_name("type")
            && type_contains_condvar(ty, source)
        {
            return true;
        }
    }
    false
}

fn type_contains_condvar(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "type_identifier"
        && node.utf8_text(source) == Ok("Condvar")
    {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| type_contains_condvar(child, source))
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
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    fn run_at(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, path)
    }

    #[test]
    fn flags_mutex_bool() {
        let diags = run("static ERRORED: Mutex<bool> = Mutex::new(false);");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicBool"));
    }

    #[test]
    fn flags_mutex_usize() {
        let diags = run("static COUNT: Mutex<usize> = Mutex::new(0);");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicUsize"));
    }

    #[test]
    fn flags_mutex_u64() {
        let diags = run("static COUNTER: Mutex<u64> = Mutex::new(0);");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicU64"));
    }

    #[test]
    fn allows_mutex_string() {
        assert!(run("static DATA: Mutex<String> = Mutex::new(String::new());").is_empty());
    }

    #[test]
    fn allows_mutex_vec() {
        assert!(run("static DATA: Mutex<Vec<u8>> = Mutex::new(Vec::new());").is_empty());
    }

    #[test]
    fn allows_atomic_bool() {
        assert!(run("static ERRORED: AtomicBool = AtomicBool::new(false);").is_empty());
    }

    #[test]
    fn allows_mutex_bool_with_condvar_sibling() {
        let src = "struct LockLatch {\n    m: Mutex<bool>,\n    v: Condvar,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_named_mutex_bool_with_condvar_sibling() {
        let src = "struct Worker {\n    is_blocked: Mutex<bool>,\n    condvar: Condvar,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_with_arc_condvar_sibling() {
        let src = "struct Latch {\n    state: Mutex<usize>,\n    notify: Arc<Condvar>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_with_qualified_condvar_sibling() {
        let src = "struct Latch {\n    state: Mutex<bool>,\n    cv: std::sync::Condvar,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_mutex_bool_without_condvar_sibling() {
        let src = "struct State {\n    flag: Mutex<bool>,\n    name: String,\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicBool"));
    }

    #[test]
    fn flags_lone_mutex_usize_field() {
        let src = "struct Counter {\n    count: Mutex<usize>,\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicUsize"));
    }

    #[test]
    fn allows_mutex_primitive_in_cfg_test_module() {
        let src = "#[cfg(test)]\nmod tests {\n    type TestT = Arc<Mutex<u32>>;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_primitive_in_integration_tests_dir() {
        // xh FP (#4395): integration-test support module under `tests/` has no
        // `#[cfg(test)]` attribute, so it is exempted by path, not by attribute.
        let src = "pub struct Server {\n    successful_hits: Arc<Mutex<u8>>,\n}";
        assert!(run_at(src, "tests/server/mod.rs").is_empty());
    }

    #[test]
    fn flags_mutex_primitive_field_in_src() {
        // The tests-dir exemption is path-scoped: the same field in `src/`
        // production code must still be flagged.
        let src = "pub struct Server {\n    successful_hits: Mutex<u8>,\n}";
        let diags = run_at(src, "src/lib.rs");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicU8"));
    }

    #[test]
    fn allows_field_with_clippy_mutex_atomic_allow() {
        // winit FP (#4504): a field-level `#[allow(clippy::mutex_atomic)]`.
        let src = "struct SharedStateX11 {\n    #[allow(clippy::mutex_atomic)]\n    cursor_visible: Mutex<bool>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_struct_with_clippy_mutex_atomic_allow() {
        let src = "#[allow(clippy::mutex_atomic)]\nstruct S {\n    x: Mutex<bool>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_field_with_clippy_mutex_atomic_expect() {
        let src = "struct S {\n    #[expect(clippy::mutex_atomic)]\n    x: Mutex<bool>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_field_with_unrelated_clippy_allow() {
        // A *different* clippy lint allow must not suppress `mutex_atomic`.
        let src = "struct S {\n    #[allow(clippy::other_lint)]\n    x: Mutex<bool>,\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicBool"));
    }

    #[test]
    fn skips_tokio_sync_mutex_via_use() {
        // openobserve FP (#7753): `tokio::sync::Mutex<bool>` is an async mutex
        // with no `AtomicBool` equivalent.
        let src = "use tokio::sync::Mutex;\nstruct LockHolder {\n    lock: Arc<Mutex<bool>>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_tokio_sync_mutex_via_grouped_use() {
        let src = "use tokio::sync::{Mutex, MutexGuard, RwLock};\nstruct S {\n    m: Mutex<bool>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_aliased_tokio_sync_mutex() {
        let src = "use tokio::sync::Mutex as TokioMutex;\nstruct S {\n    m: TokioMutex<bool>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_qualified_futures_mutex() {
        let src = "struct S {\n    f: futures::lock::Mutex<bool>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_qualified_tokio_mutex() {
        let src = "struct S {\n    t: tokio::sync::Mutex<u8>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_async_std_mutex_via_use() {
        let src = "use async_std::sync::Mutex;\nstruct S {\n    m: Mutex<bool>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_qualified_async_lock_mutex() {
        let src = "struct S {\n    m: async_lock::Mutex<bool>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_bare_mutex_under_async_glob_import() {
        let src = "use tokio::sync::*;\nstruct S {\n    m: Mutex<bool>,\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_std_sync_mutex_via_use() {
        let src = "use std::sync::Mutex;\nstruct S {\n    m: Mutex<bool>,\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicBool"));
    }

    #[test]
    fn flags_parking_lot_mutex_via_use() {
        let src = "use parking_lot::Mutex;\nstruct S {\n    m: Mutex<u32>,\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicU32"));
    }

    #[test]
    fn flags_qualified_std_sync_mutex() {
        let src = "struct S {\n    m: std::sync::Mutex<bool>,\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicBool"));
    }

    #[test]
    fn flags_bare_mutex_without_async_import() {
        // No async-mutex `use` in the file: the bare `Mutex` keeps flagging.
        let src = "struct S {\n    m: Mutex<bool>,\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicBool"));
    }

    #[test]
    fn flags_std_mutex_beside_async_glob_import() {
        // An explicit `use std::sync::Mutex` binding wins over a co-present
        // `use tokio::sync::*` glob, so the primitive `Mutex` still flags.
        let src = "use std::sync::Mutex;\nuse tokio::sync::*;\nstruct S {\n    m: Mutex<bool>,\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicBool"));
    }

    #[test]
    fn flags_non_async_type_aliased_to_mutex() {
        // `use foo::Bar as Mutex` binds the name `Mutex` to a non-async module,
        // so the primitive `Mutex` still flags — the alias resolves by module.
        let src = "use foo::Bar as Mutex;\nstruct S {\n    m: Mutex<bool>,\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicBool"));
    }
}
