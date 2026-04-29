//! tanstack-query-no-use-error-boundary backend.
//!
//! v5 renamed `useErrorBoundary` to `throwOnError`. We flag the option
//! when it appears as a property key inside any TanStack Query hook call
//! (queries and mutations both took this option in v4).

use crate::diagnostic::{Diagnostic, Severity};

const HOOKS: &[&str] = &[
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
    "useMutation",
    "queryOptions",
];

crate::ast_check! { on ["pair"] prefilter = ["useErrorBoundary"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return; };
    let Ok(key_text) = key.utf8_text(source) else { return; };
    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key_name != "useErrorBoundary" { return; }
    if !inside_hook(node, source) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`useErrorBoundary` was removed in v5 — use `throwOnError` instead.".into(),
        Severity::Warning,
    ));
}

fn inside_hook(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "call_expression"
            && let Some(func) = parent.child_by_field_name("function")
            && let Ok(name) = func.utf8_text(source)
            && HOOKS.contains(&name)
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
    fn flags() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], queryFn: f, useErrorBoundary: true })").len(),
            1
        );
    }

    #[test]
    fn allows() {
        assert!(run("useQuery({ queryKey: ['x'], queryFn: f, throwOnError: true })").is_empty());
    }
}
