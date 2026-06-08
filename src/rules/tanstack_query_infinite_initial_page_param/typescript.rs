//! tanstack-query-infinite-initial-page-param backend.
//!
//! Flags `useInfiniteQuery(...)` / `infiniteQueryOptions(...)` calls
//! whose options object is missing `initialPageParam`. v5 made the
//! starting cursor explicit — previously `undefined` was assumed and
//! passed as the first `pageParam`, but that was error-prone.

use crate::diagnostic::{Diagnostic, Severity};

const INFINITE_CALLS: &[&str] = &[
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "infiniteQueryOptions",
];

crate::ast_check! { on ["call_expression"] prefilter = ["initialPageParam"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    let Ok(func_text) = func.utf8_text(source) else { return; };
    if !INFINITE_CALLS.contains(&func_text) { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(options) = args.named_child(0) else { return; };
    if options.kind() != "object" { return; }
    if object_has_key(options, source, "initialPageParam") { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{func_text}` is missing `initialPageParam`. Required in v5 — add e.g. `initialPageParam: 0`."
        ),
        Severity::Error,
    ));
}

fn object_has_key(object: tree_sitter::Node<'_>, source: &[u8], needle: &str) -> bool {
    let mut cursor = object.walk();
    for child in object.named_children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(key) = child.child_by_field_name("key") else {
            continue;
        };
        let Ok(raw) = key.utf8_text(source) else {
            continue;
        };
        if raw.trim_matches(|c| c == '"' || c == '\'') == needle {
            return true;
        }
    }
    false
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_missing_initial_page_param() {
        let src = "useInfiniteQuery({ queryKey: ['x'], queryFn: f, getNextPageParam: p });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_missing_on_infinite_query_options() {
        let src = "infiniteQueryOptions({ queryKey: ['x'], queryFn: f });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_initial_page_param() {
        let src = "useInfiniteQuery({ queryKey: ['x'], queryFn: f, initialPageParam: 0, getNextPageParam: p });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_regular_use_query() {
        let src = "useQuery({ queryKey: ['x'], queryFn: f });";
        assert!(run(src).is_empty());
    }
}
