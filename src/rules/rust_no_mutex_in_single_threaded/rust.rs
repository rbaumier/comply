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
//! any other reference position — a borrowed lock owned and wrapped elsewhere
//! (`&Mutex<T>` / `&RwLock<T>`, including a `Fn*(&Lock<T>)` closure-type bound),
//! a struct field — named (`struct S { m: Mutex<T> }`) or tuple
//! (`struct S(Mutex<T>)`) — a raw-pointer target (`*const Mutex<T>` /
//! `*mut Mutex<T>`, a transient unsafe alias such as the
//! `Arc::into_raw`/`Arc::from_raw` view of an `Arc<Mutex<T>>` allocation) and a
//! turbofish type argument in expression
//! position (`v.downcast_ref::<Mutex<T>>()`, `size_of::<Mutex<T>>()`), which
//! only names the type a generic fn operates on. None can be `Arc`-wrapped at
//! that site: the lock is owned or wrapped elsewhere.
//!
//! Rayon parallelism counts as cross-thread sharing: when the enclosing
//! function contains a rayon parallel construct (`.par_iter()`,
//! `.par_iter_mut()`, `.into_par_iter()`, `.par_bridge()`, `.par_chunks()`,
//! `.par_chunks_mut()`, `.par_extend()`, `.par_sort*()`, `rayon::join`,
//! `rayon::scope`, `rayon::spawn`), worker threads access the lock
//! concurrently — no `Arc` appears because rayon borrows the container across
//! threads — so the `Mutex`/`RwLock` is justified and stays unflagged.
//!
//! A bare `Mutex::new(..)` also escapes the current thread when it is moved
//! into an `Arc` at the construction site: either as a local later wrapped
//! (`let m = Mutex::new(..); Arc::new(m)`) or inline as a struct-literal field
//! initializer whose surrounding literal is the argument of `Arc::new`
//! (`Arc::new(S { m: Mutex::new(..), .. })`, including nested struct literals).
//! Both are genuinely shared, so the lock stays unflagged.
//!
//! Test code is exempted — tests commonly use bare `Mutex` for simple
//! state without thread sharing, and rewriting them to `RefCell`
//! wouldn't catch a real bug.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;

const SHARING_WRAPPERS: &[&str] = &["Arc", "Rc", "LazyLock", "OnceLock", "Lazy", "OnceCell"];

/// Constructors that move their whole argument into a shared, cross-thread
/// wrapper. Both `local_mutex_is_shared_later` (the `let m = Mutex::new();
/// Arc::new(m)` flow) and `mutex_in_arc_wrapped_struct_literal` (the inline
/// `Arc::new(S { m: Mutex::new(..) })` flow) key on this set.
const ARC_CONSTRUCTORS: &[&str] =
    &["Arc::new", "std::sync::Arc::new", "alloc::sync::Arc::new"];

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
            // A reference (`&Mutex<T>` / `&RwLock<T>`) borrows a lock owned and
            // wrapped elsewhere; it cannot be `Arc`-wrapped at the borrow site.
            // This generalizes the `parameter` and `&Mutex`-return exemptions to
            // every borrowed-lock position — most importantly a `Fn*(&Lock<T>)`
            // closure-type bound, whose parameters live in a `function_type`
            // rather than a `parameter` node and so match no other arm. The walk
            // visits ancestors only, so an owned `Mutex<&T>` whose inner type
            // merely contains a reference is unaffected.
            "reference_type" => return,
            // A raw-pointer target (`*const Mutex<T>` / `*mut Mutex<T>`) is a
            // transient unsafe alias that cannot syntactically carry the `Arc`
            // wrapper, so a `Mutex` reached through a `pointer_type` is not a
            // bare single-threaded lock — ownership lives wherever the pointer
            // was derived from. The canonical case is the `Arc::into_raw` /
            // `Arc::from_raw` view of an `Arc<Mutex<T>>` allocation.
            "pointer_type" => return,
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
    if mutex_in_arc_wrapped_struct_literal(node, source) {
        return;
    }
    if enclosing_fn_uses_rayon(node, source) {
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

/// `Mutex::new(..)` written inline as a struct-literal field initializer is
/// shared across threads when the surrounding struct literal is moved into a
/// recognized sharing wrapper at the construction site
/// (`Arc::new(S { m: Mutex::new(..), .. })`) — the inline twin of the
/// `let m = Mutex::new(); Arc::new(m)` data flow `local_mutex_is_shared_later`
/// already exempts. Ascends from the lock to the enclosing `struct_expression`,
/// chaining through nested struct literals, and returns true once a literal in
/// that chain is the argument of an `Arc::new` call.
fn mutex_in_arc_wrapped_struct_literal(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(mut struct_expr) = enclosing_struct_literal_of_field(node) else {
        return false;
    };
    loop {
        let Some(parent) = struct_expr.parent() else {
            return false;
        };
        match parent.kind() {
            "arguments" => return call_is_arc_new(parent, source),
            // The struct literal is itself a field value of an outer literal;
            // climb to that outer literal and re-check.
            "field_initializer" => {
                let Some(outer) = enclosing_struct_literal_of_field(struct_expr) else {
                    return false;
                };
                struct_expr = outer;
            }
            _ => return false,
        }
    }
}

/// From a node inside a struct-literal field value, return the enclosing
/// `struct_expression` (`field_initializer` → `field_initializer_list` →
/// `struct_expression`). Returns `None` when the node is not directly inside a
/// struct-literal field, stopping at a `function_item`/`closure_expression`
/// boundary so a local in a closure stored as a field is not mistaken for the
/// field value itself.
fn enclosing_struct_literal_of_field(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cur = node.parent();
    while let Some(parent) = cur {
        match parent.kind() {
            "field_initializer" => {
                return parent
                    .parent()
                    .and_then(|list| list.parent())
                    .filter(|s| s.kind() == "struct_expression");
            }
            "function_item" | "closure_expression" => return None,
            _ => {}
        }
        cur = parent.parent();
    }
    None
}

/// Whether the `call_expression` owning `arguments` calls a recognized
/// `Arc::new` constructor.
fn call_is_arc_new(arguments: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(call) = arguments.parent() else {
        return false;
    };
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    func.utf8_text(source)
        .map(|text| ARC_CONSTRUCTORS.contains(&text))
        .unwrap_or(false)
}

/// Rayon `ParallelIterator` / `IntoParallelIterator` method entry points.
/// Matched as `.<name>(` so a same-named field or free function does not count.
const RAYON_PAR_METHODS: &[&str] = &[
    "par_iter",
    "par_iter_mut",
    "into_par_iter",
    "par_bridge",
    "par_chunks",
    "par_chunks_mut",
    "par_extend",
    "par_sort",
];

/// Rayon free-function scopes that run their body on worker threads.
const RAYON_SCOPE_FNS: &[&str] = &["rayon::join", "rayon::scope", "rayon::spawn"];

/// A `Mutex`/`RwLock` inside a function that drives a rayon parallel construct
/// is accessed concurrently by worker threads — rayon borrows the container
/// across threads without `Arc`, so the lock is justified. Scope is the
/// enclosing function body (the unit the rule's other heuristics also use);
/// when there is no enclosing function, the whole file is scanned.
fn enclosing_fn_uses_rayon(node: tree_sitter::Node, source: &[u8]) -> bool {
    let whole_file = std::str::from_utf8(source).unwrap_or("");
    let scope = enclosing_fn_text(node, source).unwrap_or(whole_file);
    text_uses_rayon(scope)
}

fn enclosing_fn_text<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cur = node.parent();
    while let Some(parent) = cur {
        // Scope to the full enclosing function, not the nearest closure: a lock
        // created inside a rayon closure (`par_iter().for_each(|_| Mutex::new(..))`)
        // is still driven by the `.par_iter()` on the function-level receiver.
        if parent.kind() == "function_item" {
            return parent.utf8_text(source).ok();
        }
        cur = parent.parent();
    }
    None
}

fn text_uses_rayon(text: &str) -> bool {
    for method in RAYON_PAR_METHODS {
        // `par_sort` also covers `par_sort_unstable`/`par_sort_by`; matching the
        // `.par_sort` prefix without the trailing `(` keeps those variants in.
        let needle = if *method == "par_sort" {
            format!(".{method}")
        } else {
            format!(".{method}(")
        };
        if text.contains(&needle) {
            return true;
        }
    }
    RAYON_SCOPE_FNS.iter().any(|f| text.contains(f))
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
    ARC_CONSTRUCTORS
        .iter()
        .any(|ctor| text.contains(&format!("{ctor}({name}")))
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
    fn allows_rwlock_ref_in_fnmut_trait_bound() {
        let src = r#"
fn proxy_all_segments_and_apply<F>(mut operation: F) -> OperationResult<()>
where
    F: FnMut(&RwLock<dyn StorageSegmentEntry>) -> OperationResult<()>,
{ }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_ref_in_fn_trait_bound() {
        assert!(run("fn apply<F: Fn(&Mutex<u32>)>(f: F) {}").is_empty());
    }

    #[test]
    fn allows_mutex_ref_struct_field() {
        assert!(run("struct S<'a> { lock: &'a Mutex<u32> }").is_empty());
    }

    #[test]
    fn flags_owned_mutex_of_reference() {
        assert_eq!(run("fn f() { let m: Mutex<&u32> = todo!(); }").len(), 1);
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

    #[test]
    fn allows_vec_mutex_accessed_in_par_iter() {
        let src = r#"
fn kmeans(nodes: &[Vec<f32>], k: usize) {
    let cluster_count: Vec<Mutex<usize>> = (0..k).map(|_| Mutex::new(0)).collect();
    nodes.par_iter().zip(0..nodes.len()).for_each(|(node, _j)| {
        *cluster_count[0].lock().unwrap() += 1;
    });
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_created_inside_par_iter_closure() {
        let src = "fn f(v: &[u32]) { v.par_iter().for_each(|x| { let m: Mutex<u32> = Mutex::new(*x); let _g = m.lock().unwrap(); }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_with_par_iter_mut() {
        let src = "fn f(v: &mut [u32]) { let m: Mutex<u32> = Mutex::new(0); v.par_iter_mut().for_each(|x| { *m.lock().unwrap() += *x; }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_with_into_par_iter() {
        let src = "fn f(v: Vec<u32>) { let m: Mutex<u32> = Mutex::new(0); v.into_par_iter().for_each(|x| { *m.lock().unwrap() += x; }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_with_rayon_scope() {
        let src = "fn f() { let m: Mutex<u32> = Mutex::new(0); rayon::scope(|s| { s.spawn(|_| { *m.lock().unwrap() += 1; }); }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_with_par_sort_variant() {
        let src = "fn f(v: &mut [u32]) { let m: Mutex<u32> = Mutex::new(0); let _ = &m; v.par_sort_unstable(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_bare_mutex_in_fn_without_rayon() {
        let src = "fn f() { let m: Mutex<u32> = Mutex::new(0); let _g = m.lock().unwrap(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_mutex_with_plain_iter_not_par() {
        let src = "fn f(v: &[u32]) { let m: Mutex<u32> = Mutex::new(0); v.iter().for_each(|x| { *m.lock().unwrap() += *x; }); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_const_ptr_to_mutex_from_into_raw() {
        let src = "fn into_overlapped(sock_state: Pin<Arc<Mutex<SockState>>>) -> *mut c_void { let overlapped_ptr: *const Mutex<SockState> = unsafe { Arc::into_raw(Pin::into_inner_unchecked(sock_state)) }; overlapped_ptr as *mut _ }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_const_ptr_to_mutex_from_cast() {
        let src = "fn from_overlapped(ptr: *mut OVERLAPPED) -> Pin<Arc<Mutex<SockState>>> { let sock_ptr: *const Mutex<SockState> = ptr as *const _; unsafe { Pin::new_unchecked(Arc::from_raw(sock_ptr)) } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mut_ptr_to_mutex_local() {
        assert!(run("fn f() { let p: *mut Mutex<u32> = std::ptr::null_mut(); let _ = p; }").is_empty());
    }

    #[test]
    fn flags_direct_mutex_not_behind_pointer() {
        let src = "fn f() { let m: Mutex<SockState> = Mutex::new(state); let _g = m.lock().unwrap(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn rayon_in_sibling_fn_does_not_exempt() {
        let src = r#"
fn a(v: &[u32]) { v.par_iter().for_each(|_| {}); }
fn b() { let m: Mutex<u32> = Mutex::new(0); let _g = m.lock().unwrap(); }
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_mutex_turbofish_in_arc_wrapped_struct_literal() {
        let src = r#"
fn new() {
    let interest = Arc::new(Registration {
        interest: Mutex::<Option<Interest>>::new(None),
        end: PipeEnd::Reader,
    });
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutex_turbofish_in_nested_arc_wrapped_struct_literal() {
        let src = "fn f() { let s = Arc::new(Outer { inner: Inner { m: Mutex::<u32>::new(0) } }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_mutex_turbofish_in_unwrapped_struct_literal() {
        let src = "fn f() { let r = Registration { interest: Mutex::<u32>::new(0) }; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_mutex_turbofish_in_struct_literal_passed_to_non_arc_call() {
        let src = "fn f() { sink(Registration { interest: Mutex::<u32>::new(0) }); }";
        assert_eq!(run(src).len(), 1);
    }
}
