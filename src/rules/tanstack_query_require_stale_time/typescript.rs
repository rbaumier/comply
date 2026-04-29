//! tanstack-query-require-stale-time backend.
//!
//! Flag `new QueryClient(...)` whose argument tree contains no
//! `staleTime` key. Without one, every component mount refetches.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] prefilter = ["QueryClient"] => |node, source, ctx, diagnostics|
    let Some(constructor) = node.child_by_field_name("constructor") else { return; };
    let Ok(name) = constructor.utf8_text(source) else { return; };
    if name != "QueryClient" { return; }
    if subtree_has_key(node, source, "staleTime") { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`QueryClient` without a default `staleTime` refetches on every component mount.".into(),
        Severity::Warning,
    ));
}

/// True if any descendant `pair` has a key whose unquoted text is `name`.
fn subtree_has_key(root: tree_sitter::Node<'_>, source: &[u8], name: &str) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "pair"
            && let Some(key) = n.child_by_field_name("key")
            && let Ok(text) = key.utf8_text(source)
            && text.trim_matches(|c| c == '"' || c == '\'') == name
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
    fn flags_no_stale_time() {
        assert_eq!(run("const client = new QueryClient({})").len(), 1);
    }

    #[test]
    fn allows_stale_time() {
        assert!(run(
            "const client = new QueryClient({ defaultOptions: { queries: { staleTime: 60_000 } } })"
        )
        .is_empty());
    }
}
