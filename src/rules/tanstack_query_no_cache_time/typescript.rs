//! tanstack-query-no-cache-time backend.
//!
//! Flag any object property literally named `cacheTime` inside a call
//! whose callee is `QueryClient` (constructor) or one of the query hooks.
//! In TanStack Query v5 the option was renamed to `gcTime`.

use crate::diagnostic::{Diagnostic, Severity};

const SCOPES: &[&str] = &[
    "QueryClient",
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
    "queryOptions",
];

crate::ast_check! { on ["pair"] prefilter = ["cacheTime"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return; };
    let Ok(key_text) = key.utf8_text(source) else { return; };
    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key_name != "cacheTime" { return; }
    if !inside_known_scope(node, source) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &key,
        super::META.id,
        "`cacheTime` was renamed to `gcTime` in TanStack Query v5.".into(),
        Severity::Warning,
    ));
}

fn inside_known_scope(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        let kind = parent.kind();
        if kind == "call_expression" || kind == "new_expression" {
            if let Some(func) = parent
                .child_by_field_name("function")
                .or_else(|| parent.child_by_field_name("constructor"))
                && let Ok(name) = func.utf8_text(source)
                && SCOPES.contains(&name)
            {
                return true;
            }
            // Some grammars expose `new_expression` with constructor as
            // a named child rather than a field.
            if kind == "new_expression"
                && let Some(c) = parent.named_child(0)
                && let Ok(name) = c.utf8_text(source)
                && SCOPES.contains(&name)
            {
                return true;
            }
        }
        current = parent;
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
    fn flags_cache_time() {
        assert_eq!(
            run("new QueryClient({ defaultOptions: { queries: { cacheTime: 5000 } } })").len(),
            1
        );
    }

    #[test]
    fn allows_gc_time() {
        assert!(
            run("new QueryClient({ defaultOptions: { queries: { gcTime: 5000 } } })").is_empty()
        );
    }
}
