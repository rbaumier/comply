//! tanstack-query-max-pages-requires-both backend.
//!
//! Flags `useInfiniteQuery({ maxPages: N, ... })` where either
//! `getNextPageParam` or `getPreviousPageParam` is missing. TanStack
//! Query enforces bidirectional paging semantics when `maxPages` is
//! set: it must be able to re-paginate in both directions.

use crate::diagnostic::{Diagnostic, Severity};

const INFINITE_CALLS: &[&str] = &[
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "infiniteQueryOptions",
];

crate::ast_check! { on ["call_expression"] prefilter = ["maxPages"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    let Ok(func_text) = func.utf8_text(source) else { return; };
    if !INFINITE_CALLS.contains(&func_text) { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(options) = args.named_child(0) else { return; };
    if options.kind() != "object" { return; }

    if !has_key(options, source, "maxPages") { return; }

    let has_next = has_key(options, source, "getNextPageParam");
    let has_prev = has_key(options, source, "getPreviousPageParam");
    if has_next && has_prev { return; }

    let missing = match (has_next, has_prev) {
        (false, false) => "`getNextPageParam` and `getPreviousPageParam`",
        (false, true) => "`getNextPageParam`",
        (true, false) => "`getPreviousPageParam`",
        _ => unreachable!(),
    };

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`maxPages` is set on `{func_text}` but {missing} is missing. Both page-param functions are required."
        ),
        Severity::Error,
    ));
}

fn has_key(object: tree_sitter::Node<'_>, source: &[u8], needle: &str) -> bool {
    let mut cursor = object.walk();
    for child in object.named_children(&mut cursor) {
        if child.kind() != "pair" { continue; }
        let Some(key) = child.child_by_field_name("key") else { continue; };
        let Ok(raw) = key.utf8_text(source) else { continue; };
        if raw.trim_matches(|c| c == '"' || c == '\'') == needle {
            return true;
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
    fn flags_max_pages_without_previous() {
        let src = "useInfiniteQuery({ queryKey: ['x'], queryFn: f, initialPageParam: 0, getNextPageParam: n, maxPages: 5 });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_max_pages_without_next() {
        let src = "useInfiniteQuery({ queryKey: ['x'], queryFn: f, initialPageParam: 0, getPreviousPageParam: p, maxPages: 5 });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_max_pages_with_both() {
        let src = "useInfiniteQuery({ queryKey: ['x'], queryFn: f, initialPageParam: 0, getNextPageParam: n, getPreviousPageParam: p, maxPages: 5 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_missing_max_pages() {
        let src = "useInfiniteQuery({ queryKey: ['x'], queryFn: f, initialPageParam: 0, getNextPageParam: n });";
        assert!(run(src).is_empty());
    }
}
