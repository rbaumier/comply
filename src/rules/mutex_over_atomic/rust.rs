use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_test_context, is_suppressed_by_clippy_allow};

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
    // lock-free-atomic discipline only applies to production code.
    if is_in_test_context(node, source) { return; }

    let Ok(text) = node.utf8_text(source) else { return };
    if text != "Mutex" { return; }

    let Some(parent) = node.parent() else { return };
    if parent.kind() != "generic_type" { return; }

    // A `Mutex` paired with a `Condvar` sibling field is the condition-variable
    // latch pattern: `Condvar::wait` takes a `MutexGuard`, so the `Mutex` is
    // structurally required and has no atomic equivalent. Skip it.
    if has_condvar_sibling_field(parent, source) {
        return;
    }

    // `#[allow(clippy::mutex_atomic)]` / `#[expect(...)]` on the field or its
    // enclosing struct is the author explicitly overriding the equivalent
    // clippy lint (e.g. the value is guarded together with sibling fields under
    // one lock). Honor that suppression like the other clippy-mirroring rules.
    if is_suppressed_by_clippy_allow(node, &["mutex_atomic"], source) {
        return;
    }

    let Ok(full) = parent.utf8_text(source) else { return };

    for &(prim, atomic) in ATOMIC_TYPES {
        let pattern = format!("Mutex<{prim}>");
        if full == pattern {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &parent,
                super::META.id,
                format!("`{pattern}` — prefer `{atomic}` for lock-free access."),
                Severity::Warning,
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
}
