//! tanstack-query-no-is-loading backend.
//!
//! Flag references to the `isLoading` name in files that also call a
//! TanStack Query hook. v5 renamed the flag to `isPending`; an unrenamed
//! `isLoading` in a query consumer is almost always a v4 leftover.

use crate::diagnostic::{Diagnostic, Severity};

const HOOKS: &[&str] = &[
    "useQuery",
    "useInfiniteQuery",
    "useSuspenseQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
];

crate::ast_check! { on ["identifier", "property_identifier", "shorthand_property_identifier_pattern", "shorthand_property_identifier"] => |node, source, ctx, diagnostics|
    // Match identifier-like nodes whose text is `isLoading`. We accept
    // `identifier`, `property_identifier`, and the shorthand pattern node
    // emitted by destructuring (`{ isLoading }`).
    let Ok(text) = node.utf8_text(source) else { return; };
    if text != "isLoading" { return; }
    if !file_uses_query_hook(node, source) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`isLoading` was removed in TanStack Query v5 — use `isPending` instead.".into(),
        Severity::Warning,
    ));
}

/// True when any descendant of the file root is a call to a query hook.
/// We look up to the program root once per match.
fn file_uses_query_hook(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(func) = n.child_by_field_name("function")
            && let Ok(name) = func.utf8_text(source)
            && HOOKS.contains(&name)
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
    fn flags_is_loading() {
        assert!(!run("const { isLoading } = useQuery({ queryKey: ['x'], queryFn: f })").is_empty());
    }

    #[test]
    fn allows_is_pending() {
        assert!(run("const { isPending } = useQuery({ queryKey: ['x'], queryFn: f })").is_empty());
    }

    #[test]
    fn ignores_file_without_usequery() {
        assert!(run("const isLoading = true").is_empty());
    }
}
