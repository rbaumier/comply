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
            _ => {}
        }
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
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
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
}
