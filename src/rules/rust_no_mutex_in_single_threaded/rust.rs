//! rust-no-mutex-in-single-threaded backend.
//!
//! Walks `generic_type` nodes whose base type name is `Mutex`
//! (std, tokio, or parking_lot — matched via the trailing `::` segment).
//! A `Mutex<T>` is considered single-threaded and flagged unless one of
//! its ancestor `generic_type` nodes is a recognised sharing wrapper
//! (`Arc`, `Rc`, `LazyLock`, `OnceLock`, `Lazy`, `OnceCell`).
//!
//! Non-usage positions are exempt: a type alias definition
//! (`type Lists<T> = Mutex<…>;`), the subject of an `impl` head
//! (`impl<T> RwLock<T>`, `unsafe impl<T> Send for RwLock<T>`), a function
//! return type (`fn new() -> RwLock<T>`, `fn get(&self) -> &Mutex<T>`), a
//! function parameter (`fn f(m: &Mutex<T>)` borrows a lock the caller owns),
//! a struct field — named (`struct S { m: Mutex<T> }`) or tuple
//! (`struct S(Mutex<T>)`) — and a turbofish type argument in expression
//! position (`v.downcast_ref::<Mutex<T>>()`, `size_of::<Mutex<T>>()`), which
//! only names the type a generic fn operates on. None can be `Arc`-wrapped at
//! that site: the lock is owned or wrapped elsewhere.
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
            "field_declaration" | "field_declaration_list"
            | "ordered_field_declaration_list" => return,
            "static_item" | "const_item" => return,
            // A function parameter (`fn f(m: &Mutex<T>)`) borrows a lock the
            // caller owns and constructs; the callee can't `Arc`-wrap it, the
            // owner does so at the lock's definition. Trait-method-signature
            // params are also `parameter` nodes, so this covers them too. A
            // `Mutex<T>` in the function body is reached through `body`, not
            // `parameter`, and stays flagged.
            "parameter" => return,
            // A turbofish / generic argument in EXPRESSION position
            // (`downcast_ref::<Mutex<T>>()`, `size_of::<Mutex<T>>()`) only NAMES
            // the type — it is not an owning binding and cannot be `Arc`-wrapped
            // here. A type argument nested inside another TYPE (`Vec<Mutex<T>>`,
            // `Arc<Mutex<T>>`) IS a real usage, so keep walking (the
            // `generic_type` arm handles the `Arc`-wrapper exemption).
            "type_arguments" => {
                if ancestor.parent().map(|p| p.kind()) != Some("generic_type") {
                    return;
                }
            }
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
            // A function return type (`fn new() -> RwLock<T>`,
            // `fn get(&self) -> &Mutex<T>`) names the lock the caller receives;
            // the caller, not the callee, decides whether to share it in `Arc`,
            // so the return position can't be wrapped here. A `Mutex<T>` in the
            // function body (e.g. a `let` annotation) is reached through `body`,
            // not `return_type`, and stays flagged.
            "function_item" | "function_signature_item" => {
                if ancestor.child_by_field_name("return_type") == Some(child) {
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

    #[test]
    fn allows_rwlock_in_function_return_type() {
        assert!(run("pub fn new(value: T) -> RwLock<T> { todo!() }").is_empty());
    }

    #[test]
    fn allows_mutex_ref_in_function_return_type() {
        let src = "fn get_uring(&self) -> &Mutex<UringContext> { todo!() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_rwlock_in_trait_method_signature_return_type() {
        assert!(run("trait T { fn lock(&self) -> RwLock<u32>; }").is_empty());
    }

    #[test]
    fn allows_mutex_ref_parameter() {
        let src = "fn f(fs_watcher: &std::sync::Mutex<u32>) -> Result<(), ()> { let _g = fs_watcher.lock().unwrap(); Ok(()) }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_rwlock_ref_parameter() {
        assert!(run("fn g(m: &RwLock<u32>) {}").is_empty());
    }

    #[test]
    fn allows_lock_in_tuple_struct_field() {
        assert!(run("pub struct ResolverLock(parking_lot::RwLock<()>);").is_empty());
    }

    #[test]
    fn allows_std_mutex_in_tuple_struct_field() {
        assert!(run("struct S(std::sync::Mutex<u32>);").is_empty());
    }

    #[test]
    fn allows_mutex_in_downcast_turbofish() {
        let src = "fn get<T: Clone + 'static>(v: &dyn std::any::Any) -> T { let m = v.downcast_ref::<Mutex<T>>().expect(\"d\"); m.lock().unwrap().clone() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_in_size_of_turbofish() {
        assert!(run("fn f() -> usize { std::mem::size_of::<Mutex<u32>>() }").is_empty());
    }

    #[test]
    fn allows_rwlock_in_downcast_turbofish() {
        assert!(run("fn g(v: &dyn std::any::Any) { v.downcast_ref::<RwLock<u32>>(); }").is_empty());
    }

    #[test]
    fn flags_mutex_nested_in_vec_local() {
        assert_eq!(
            run("fn f() { let v: Vec<Mutex<u32>> = Vec::new(); let _ = &v; }").len(),
            1
        );
    }
}
