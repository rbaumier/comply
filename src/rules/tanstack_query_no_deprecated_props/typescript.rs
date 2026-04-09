//! tanstack-query-no-deprecated-props backend — flag v4-era props and names
//! removed or renamed in v5.
//!
//! The v5 renames are:
//! - `cacheTime` → `gcTime`
//! - `useErrorBoundary` → `throwOnError`
//! - `keepPreviousData: true` → `placeholderData: keepPreviousData`
//! - `onSuccess`/`onError`/`onSettled` on `useQuery` — removed entirely,
//!   use `useEffect` instead.
//!
//! Detection: walk pairs inside object literals looking for the deprecated
//! key names. We fire only inside a call whose function is a known query
//! hook (useQuery / useSuspenseQuery / useInfiniteQuery) to avoid false
//! positives on unrelated objects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const QUERY_HOOKS: &[&str] = &[
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
];

const DEPRECATED: &[(&str, &str)] = &[
    ("cacheTime", "renamed to `gcTime` in v5"),
    ("useErrorBoundary", "renamed to `throwOnError` in v5"),
    ("onSuccess", "removed from useQuery in v5 — use useEffect"),
    ("onError", "removed from useQuery in v5 — use useEffect"),
    ("onSettled", "removed from useQuery in v5 — use useEffect"),
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "pair" {
                return;
            }
            if !inside_query_hook(node, source_bytes) {
                return;
            }
            let Some(key) = node.child_by_field_name("key") else {
                return;
            };
            let Ok(key_text) = key.utf8_text(source_bytes) else {
                return;
            };
            let Some((_, reason)) = DEPRECATED.iter().find(|(k, _)| *k == key_text) else {
                return;
            };
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "tanstack-query-no-deprecated-props".into(),
                message: format!("`{key_text}` is deprecated — {reason}."),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

/// True when the pair lives inside a call expression whose callee is one
/// of the query hooks.
fn inside_query_hook(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "call_expression"
            && let Some(function) = parent.child_by_field_name("function")
            && let Ok(name) = function.utf8_text(source)
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
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.ts"), source),
            &tree,
        )
    }

    #[test]
    fn flags_cache_time() {
        assert_eq!(
            run_on("useQuery({ queryKey: ['x'], cacheTime: 5000 });").len(),
            1
        );
    }

    #[test]
    fn flags_on_success_on_use_query() {
        assert_eq!(
            run_on("useQuery({ queryKey: ['x'], onSuccess: () => {} });").len(),
            1
        );
    }

    #[test]
    fn allows_gc_time() {
        assert!(run_on("useQuery({ queryKey: ['x'], gcTime: 5000 });").is_empty());
    }

    #[test]
    fn does_not_flag_on_success_in_mutation() {
        // useMutation still supports onSuccess — don't flag it.
        assert!(
            run_on("useMutation({ onSuccess: () => {} });").is_empty()
        );
    }
}
