//! tanstack-start-require-validate-search backend — flag a single
//! `Route.useSearch()` call in a file that does not also configure a
//! `validateSearch:` option on a route. The rule is whole-file (it
//! checks the entire AST for any `validateSearch:` pair).

use crate::diagnostic::{Diagnostic, Severity};

/// Return true if `node` is a call to `Route.useSearch(...)` (member
/// expression with object identifier `Route` and property `useSearch`).
fn is_route_use_search(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "member_expression" {
        return false;
    }
    let Some(object) = function.child_by_field_name("object") else {
        return false;
    };
    let Some(property) = function.child_by_field_name("property") else {
        return false;
    };
    object.utf8_text(source).ok() == Some("Route")
        && property.utf8_text(source).ok() == Some("useSearch")
}

/// Return true if `node` is a `pair` with key `validateSearch`.
fn is_validate_search_pair(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "pair" {
        return false;
    }
    let Some(key) = node.child_by_field_name("key") else {
        return false;
    };
    let Ok(key_text) = key.utf8_text(source) else {
        return false;
    };
    let normalized = key_text.trim_matches(|c: char| c == '"' || c == '\'');
    normalized == "validateSearch"
}

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(
        &self,
        ctx: &crate::rules::backend::CheckCtx,
        tree: &tree_sitter::Tree,
    ) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();

        // Bail if the file already declares any `validateSearch:` pair.
        let pairs = crate::rules::walker::collect_nodes_of_kinds(tree, &["pair"]);
        if pairs.iter().any(|p| is_validate_search_pair(*p, source)) {
            return Vec::new();
        }

        // Find the first `Route.useSearch()` call.
        let calls = crate::rules::walker::collect_nodes_of_kinds(tree, &["call_expression"]);
        let Some(node) = calls.iter().find(|n| is_route_use_search(**n, source)) else {
            return Vec::new();
        };

        vec![Diagnostic::at_node(
            ctx.path,
            node,
            super::META.id,
            "`Route.useSearch()` without `validateSearch:` in the route config accepts untyped search params.".into(),
            Severity::Warning,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_use_search_without_validate() {
        assert_eq!(run("const { page } = Route.useSearch()").len(), 1);
    }

    #[test]
    fn allows_with_validate_search() {
        assert!(run(
            "const { page } = Route.useSearch()\nconst route = createFileRoute('/posts')({ validateSearch: z.object({ page: z.number() }) })"
        )
        .is_empty());
    }

    #[test]
    fn ignores_no_use_search() {
        assert!(run("const route = createFileRoute('/posts')({})").is_empty());
    }
}
