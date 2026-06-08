//! Detects `.clone()` on variables declared as `Arc<T>` or initialized
//! with `Arc::new(...)` / `Arc::clone(...)`.

use rustc_hash::FxHashMap;

use crate::diagnostic::{Diagnostic, Severity};

fn is_arc_binding_at_call(
    root: tree_sitter::Node,
    source: &[u8],
    call_start: usize,
    target_name: &str,
) -> bool {
    let mut bindings = FxHashMap::default();
    let mut cursor = root.walk();
    collect_bindings_before_call(root, source, call_start, &mut cursor, &mut bindings);
    bindings.get(target_name).copied().unwrap_or(false)
}

fn collect_bindings_before_call<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
    call_start: usize,
    cursor: &mut tree_sitter::TreeCursor<'a>,
    bindings: &mut FxHashMap<&'a str, bool>,
) {
    if node.start_byte() >= call_start {
        return;
    }
    if node.kind() == "let_declaration"
        && let Some((name, is_arc)) = binding_arc_state(node, source)
    {
        bindings.insert(name, is_arc);
    }
    if cursor.goto_first_child() {
        loop {
            collect_bindings_before_call(cursor.node(), source, call_start, cursor, bindings);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

fn binding_arc_state<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<(&'a str, bool)> {
    let pattern = node.child_by_field_name("pattern")?;
    if pattern.kind() != "identifier" {
        return None;
    }
    let name = pattern.utf8_text(source).ok()?;
    let has_arc_type = node
        .child_by_field_name("type")
        .is_some_and(|t| is_arc_type_text(t.utf8_text(source).unwrap_or("")));
    let has_arc_init = node
        .child_by_field_name("value")
        .is_some_and(|v| is_arc_init_text(v.utf8_text(source).unwrap_or("")));
    Some((name, has_arc_type || has_arc_init))
}

fn is_arc_type_text(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    compact.starts_with("Arc<") || compact.contains("::Arc<")
}

fn is_arc_init_text(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    for prefix in [
        "Arc::new(",
        "Arc::clone(",
        "std::sync::Arc::new(",
        "std::sync::Arc::clone(",
        "alloc::sync::Arc::new(",
        "alloc::sync::Arc::clone(",
    ] {
        if compact.starts_with(prefix) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] prefilter = ["clone"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "field_expression" { return; }
    let Some(field) = func.child_by_field_name("field") else { return };
    if field.utf8_text(source).unwrap_or("") != "clone" { return; }
    let Some(object) = func.child_by_field_name("value") else { return; };
    if object.kind() != "identifier" { return; }
    let obj_name = object.utf8_text(source).unwrap_or("");

    let root = node.parent().map(|n| {
        let mut r = n;
        while let Some(p) = r.parent() { r = p; }
        r
    }).unwrap_or(node);

    if !is_arc_binding_at_call(root, source, node.start_byte(), obj_name) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`{obj_name}.clone()` — use `Arc::clone(&{obj_name})` to signal a cheap ref-count bump."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_clone_on_arc_typed() {
        let src = "fn f() { let x: Arc<String> = Arc::new(String::new()); let y = x.clone(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_clone_on_arc_inferred() {
        let src = "fn f() { let x = Arc::new(42); let y = x.clone(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_arc_clone_call() {
        let src = "fn f() { let x = Arc::new(42); let y = Arc::clone(&x); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_clone_on_non_arc() {
        let src = "fn f() { let x = String::new(); let y = x.clone(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_clone_before_arc_binding() {
        let src = "fn f() { let y = x.clone(); let x = Arc::new(42); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_shadowed_non_arc_binding() {
        let src = "fn f() { let x = Arc::new(42); let x = String::new(); let y = x.clone(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_std_sync_arc_new() {
        let src = "fn f() { let x = std::sync::Arc::new(42); let y = x.clone(); }";
        assert_eq!(run(src).len(), 1);
    }
}
