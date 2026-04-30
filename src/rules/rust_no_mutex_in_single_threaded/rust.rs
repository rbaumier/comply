//! rust-no-mutex-in-single-threaded backend.
//!
//! Walks `generic_type` nodes whose base type name is `Mutex`
//! (std, tokio, or parking_lot — matched via the trailing `::` segment).
//! A `Mutex<T>` is considered single-threaded and flagged unless one of
//! its ancestor `generic_type` nodes is a recognised sharing wrapper
//! (`Arc`, `Rc`, `LazyLock`, `OnceLock`, `Lazy`, `OnceCell`).
//!
//! Test code is exempted — tests commonly use bare `Mutex` for simple
//! state without thread sharing, and rewriting them to `RefCell`
//! wouldn't catch a real bug.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;

const SHARING_WRAPPERS: &[&str] = &["Arc", "Rc", "LazyLock", "OnceLock", "Lazy", "OnceCell"];

crate::ast_check! { on ["generic_type"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    if is_in_test_context(node, source) { return; }

    let Some(type_node) = node.child_by_field_name("type") else { return; };
    let type_text = type_node.utf8_text(source).unwrap_or("");
    let base = type_text.rsplit("::").next().unwrap_or("");
    if base != "Mutex" && base != "RwLock" { return; }

    // Walk ancestors: if any enclosing `generic_type` is a known sharing
    // wrapper, the Mutex is thread-shared by construction.
    let mut cur = node.parent();
    while let Some(ancestor) = cur {
        if ancestor.kind() == "generic_type"
            && let Some(atype) = ancestor.child_by_field_name("type")
            && let Ok(atext) = atype.utf8_text(source)
        {
            let abase = atext.rsplit("::").next().unwrap_or("");
            if SHARING_WRAPPERS.contains(&abase) {
                return;
            }
        }
        cur = ancestor.parent();
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{base}<T>` without `Arc<{base}<T>>` locks atomically for no cross-thread sharing. \
             Use `RefCell<T>` for single-threaded interior mutability, or wrap in `Arc` if threads are actually involved."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_bare_mutex_field() {
        assert_eq!(run("struct S { state: Mutex<u32> }").len(), 1);
    }

    #[test]
    fn flags_std_sync_mutex() {
        assert_eq!(run("struct S { state: std::sync::Mutex<u32> }").len(), 1);
    }

    #[test]
    fn flags_bare_rwlock() {
        assert_eq!(run("struct S { map: RwLock<HashMap<u32, u32>> }").len(), 1);
    }

    #[test]
    fn allows_arc_mutex() {
        assert!(run("struct S { state: Arc<Mutex<u32>> }").is_empty());
    }

    #[test]
    fn allows_arc_std_mutex() {
        assert!(run("struct S { state: Arc<std::sync::Mutex<u32>> }").is_empty());
    }

    #[test]
    fn allows_lazy_lock_mutex() {
        assert!(
            run("static FOO: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));").is_empty()
        );
    }

    #[test]
    fn allows_in_test_context() {
        assert!(run("#[cfg(test)]\nmod tests { struct S { m: Mutex<u32> } }").is_empty());
    }
}
