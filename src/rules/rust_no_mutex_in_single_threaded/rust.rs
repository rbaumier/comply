//! rust-no-mutex-in-single-threaded backend.
//!
//! Walks `generic_type` nodes whose base type name is `Mutex`
//! (std, tokio, or parking_lot — matched via the trailing `::` segment).
//! A `Mutex<T>` is considered single-threaded and flagged unless one of
//! its ancestor `generic_type` nodes is a recognised sharing wrapper
//! (`Arc`, `Rc`, `LazyLock`, `OnceLock`, `Lazy`, `OnceCell`).
//!
//! Non-usage positions are exempt: a type alias definition
//! (`type Lists<T> = Mutex<…>;`) and the subject of an `impl` head
//! (`impl<T> RwLock<T>`, `unsafe impl<T> Send for RwLock<T>`) name the lock
//! type rather than use it, so neither can be `Arc`-wrapped at that site.
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

    let mut child = node;
    let mut cur = node.parent();
    while let Some(ancestor) = cur {
        match ancestor.kind() {
            "generic_type" => {
                if let Some(atype) = ancestor.child_by_field_name("type")
                    && let Ok(atext) = atype.utf8_text(source)
                {
                    let abase = atext.rsplit("::").next().unwrap_or("");
                    if SHARING_WRAPPERS.contains(&abase) {
                        return;
                    }
                }
            }
            "field_declaration" | "field_declaration_list" => return,
            "static_item" | "const_item" => return,
            // A type alias definition (`type Lists<T> = Mutex<…>;`) is not a
            // usage: callers wrap the alias in `Arc` at the use site, which the
            // alias body can't observe.
            "type_item" => return,
            // The subject of an `impl` head (`impl<T> RwLock<T>` or
            // `unsafe impl<T> Send for RwLock<T>`) is the type being
            // implemented, not a usage to wrap in `Arc`. Reaching the
            // `impl_item` through its `body` means we are inside the block and
            // looking at a genuine usage, so only exempt the head.
            "impl_item" => {
                if ancestor.child_by_field_name("body") != Some(child) {
                    return;
                }
            }
            _ => {}
        }
        child = ancestor;
        cur = ancestor.parent();
    }
    if local_mutex_is_shared_later(node, source) {
        return;
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

fn local_mutex_is_shared_later(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(name) = enclosing_let_name(node, source) else {
        return false;
    };
    let Ok(after) = std::str::from_utf8(&source[node.end_byte()..]) else {
        return false;
    };
    contains_arc_new_for(after, name)
        || (after.contains("spawn(") && contains_identifier(after, name))
}

fn enclosing_let_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cur = node.parent();
    while let Some(parent) = cur {
        if parent.kind() == "let_declaration" {
            let pattern = parent.child_by_field_name("pattern")?;
            if pattern.kind() == "identifier" {
                return pattern.utf8_text(source).ok();
            }
            return None;
        }
        cur = parent.parent();
    }
    None
}

fn contains_arc_new_for(text: &str, name: &str) -> bool {
    for prefix in [
        "Arc::new(",
        "std::sync::Arc::new(",
        "alloc::sync::Arc::new(",
    ] {
        let needle = format!("{prefix}{name}");
        if text.contains(&needle) {
            return true;
        }
    }
    false
}

fn contains_identifier(text: &str, name: &str) -> bool {
    let mut start = 0;
    while let Some(offset) = text[start..].find(name) {
        let abs = start + offset;
        let before_ok = abs == 0 || !is_ident_byte(text.as_bytes()[abs - 1]);
        let after = abs + name.len();
        let after_ok = after >= text.len() || !is_ident_byte(text.as_bytes()[after]);
        if before_ok && after_ok {
            return true;
        }
        start = abs + name.len();
    }
    false
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn allows_mutex_in_struct_field() {
        assert!(run("struct S { state: Mutex<u32> }").is_empty());
    }

    #[test]
    fn allows_std_sync_mutex_in_struct() {
        assert!(run("struct S { state: std::sync::Mutex<u32> }").is_empty());
    }

    #[test]
    fn allows_rwlock_in_struct_field() {
        assert!(run("struct S { map: RwLock<HashMap<u32, u32>> }").is_empty());
    }

    #[test]
    fn flags_bare_local_mutex() {
        assert_eq!(
            run("fn f() { let m: Mutex<u32> = Mutex::new(0); }").len(),
            1
        );
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
    fn allows_local_mutex_moved_into_arc() {
        let src = "fn f() { let m: Mutex<u32> = Mutex::new(0); let shared = Arc::new(m); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_local_mutex_moved_into_spawn() {
        let src = r#"
fn f() {
    let m: Mutex<u32> = Mutex::new(0);
    std::thread::spawn(move || {
        let _guard = m.lock().unwrap();
    });
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        assert!(run("#[cfg(test)]\nmod tests { struct S { m: Mutex<u32> } }").is_empty());
    }

    #[test]
    fn allows_mutex_type_alias() {
        assert!(run("type Lists<T> = Mutex<ListsInner<T>>;").is_empty());
    }

    #[test]
    fn allows_rwlock_type_alias() {
        assert!(run("type Lock<T> = RwLock<Inner<T>>;").is_empty());
    }

    #[test]
    fn allows_unsafe_impl_send_for_lock_type() {
        let src = "unsafe impl<T> Send for RwLock<T> where T: ?Sized + Send {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inherent_impl_on_lock_type() {
        assert!(run("impl<T: ?Sized> RwLock<T> { fn f(&self) {} }").is_empty());
    }

    #[test]
    fn flags_bare_mutex_used_inside_impl_body() {
        let src = "impl S { fn f(&self) { let m: Mutex<u32> = Mutex::new(0); } }";
        assert_eq!(run(src).len(), 1);
    }
}
