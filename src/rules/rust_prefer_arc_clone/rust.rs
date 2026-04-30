//! Detects `.clone()` on variables declared as `Arc<T>` or initialized
//! with `Arc::new(...)` / `Arc::clone(...)`.

use crate::diagnostic::{Diagnostic, Severity};

fn find_arc_bindings<'a>(root: tree_sitter::Node<'a>, source: &'a [u8]) -> Vec<&'a str> {
    let mut names = Vec::new();
    let mut cursor = root.walk();
    collect_arc_bindings(root, source, &mut cursor, &mut names);
    names
}

fn collect_arc_bindings<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
    cursor: &mut tree_sitter::TreeCursor<'a>,
    names: &mut Vec<&'a str>,
) {
    if node.kind() == "let_declaration" {
        let has_arc_type = node.child_by_field_name("type").is_some_and(|t| {
            let tt = t.utf8_text(source).unwrap_or("");
            tt.starts_with("Arc<") || tt.contains("::Arc<")
        });
        let has_arc_init = node.child_by_field_name("value").is_some_and(|v| {
            let vt = v.utf8_text(source).unwrap_or("");
            vt.starts_with("Arc::new(") || vt.starts_with("Arc::clone(")
        });
        if has_arc_type || has_arc_init {
            if let Some(pat) = node.child_by_field_name("pattern") {
                if pat.kind() == "identifier" {
                    if let Ok(name) = pat.utf8_text(source) {
                        names.push(name);
                    }
                }
            }
        }
    }
    if cursor.goto_first_child() {
        loop {
            collect_arc_bindings(cursor.node(), source, cursor, names);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
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

    let arc_bindings = find_arc_bindings(root, source);
    if !arc_bindings.contains(&obj_name) { return; }

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
}
