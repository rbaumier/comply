//! tanstack-query-prefer-query-options backend.
//!
//! Flag `useQuery({ ... })` calls whose argument is an inline object
//! literal, unless the file also uses the `queryOptions()` factory
//! somewhere (in which case the author has signalled the chosen pattern).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return; };
    let Ok(func_text) = func.utf8_text(source) else { return; };
    if func_text != "useQuery" { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(first) = args.named_child(0) else { return; };
    if first.kind() != "object" { return; }
    if file_uses_query_options(node, source) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Extract inline `useQuery` options to a `queryOptions()` factory for reuse and type-safety.".into(),
        Severity::Warning,
    ));
}

/// True if any descendant of the file root is a call to `queryOptions(...)`.
fn file_uses_query_options(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(func) = n.child_by_field_name("function")
            && let Ok(name) = func.utf8_text(source)
            && name == "queryOptions"
        {
            return true;
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_inline_options() {
        assert_eq!(
            run("useQuery({ queryKey: ['users'], queryFn: fetchUsers })").len(),
            1
        );
    }

    #[test]
    fn allows_query_options_factory() {
        assert!(run(
            "const opts = queryOptions({ queryKey: ['users'], queryFn: fetchUsers })\nuseQuery(opts)"
        )
        .is_empty());
    }
}
