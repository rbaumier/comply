//! tanstack-query-no-query-callbacks backend.
//!
//! v5 removed `onSuccess`/`onError`/`onSettled` from `useQuery` and
//! `useSuspenseQuery`. Mutations still support these callbacks, so we scope
//! detection to query hooks via the enclosing call expression.

use crate::diagnostic::{Diagnostic, Severity};

const QUERY_HOOKS: &[&str] = &[
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
];

const REMOVED: &[&str] = &["onSuccess", "onError", "onSettled"];

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return; };
    let Ok(key_text) = key.utf8_text(source) else { return; };
    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'');
    if !REMOVED.contains(&key_name) { return; }
    if !inside_query_hook(node, source) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{key_name}:` on `useQuery` was removed in TanStack Query v5 — move side-effects to `useEffect`."
        ),
        Severity::Warning,
    ));
}

fn inside_query_hook(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "call_expression"
            && let Some(func) = parent.child_by_field_name("function")
            && let Ok(name) = func.utf8_text(source)
            && QUERY_HOOKS.contains(&name)
        {
            return true;
        }
        current = parent;
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
    fn flags_on_success() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], queryFn: f, onSuccess: () => {} })").len(),
            1
        );
    }

    #[test]
    fn allows_no_callbacks() {
        assert!(run("useQuery({ queryKey: ['x'], queryFn: f })").is_empty());
    }

    #[test]
    fn ignores_use_mutation() {
        assert!(run("useMutation({ onSuccess: () => {} })").is_empty());
    }
}
