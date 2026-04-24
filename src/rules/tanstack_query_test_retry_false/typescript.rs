//! tanstack-query-test-retry-false backend.
//!
//! In test files (paths ending in `.test.ts`, `.test.tsx`, `.spec.ts`,
//! `.spec.tsx`, or under a `__tests__` segment), a `new QueryClient()`
//! without `retry: false` will silently exercise the default retry
//! logic — that turns a failing unit test into a 3x-slower test plus
//! timing flakes. Force `retry: false`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) { return; }
    if node.kind() != "new_expression" { return; }
    let Some(constructor) = node.child_by_field_name("constructor") else { return; };
    if constructor.utf8_text(source).ok() != Some("QueryClient") { return; }

    if let Some(args) = node.child_by_field_name("arguments")
        && let Some(opts) = args.named_child(0)
        && opts.kind() == "object"
        && let Some(defaults) = find_pair_value(opts, source, "defaultOptions")
        && defaults.kind() == "object"
        && let Some(queries) = find_pair_value(defaults, source, "queries")
        && queries.kind() == "object"
        && let Some(retry) = find_pair_value(queries, source, "retry")
        && retry.utf8_text(source).ok() == Some("false")
    {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Test-file `QueryClient` must set `defaultOptions.queries.retry: false` to keep tests deterministic.".into(),
        Severity::Warning,
    ));
}

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.ends_with(".test.ts")
        || s.ends_with(".test.tsx")
        || s.ends_with(".test.js")
        || s.ends_with(".test.jsx")
        || s.ends_with(".spec.ts")
        || s.ends_with(".spec.tsx")
        || s.ends_with(".spec.js")
        || s.ends_with(".spec.jsx")
        || s.contains("__tests__")
}

fn find_pair_value<'a>(
    object: tree_sitter::Node<'a>,
    source: &[u8],
    needle: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = object.walk();
    for child in object.named_children(&mut cursor) {
        if child.kind() != "pair" { continue; }
        let key = child.child_by_field_name("key")?;
        let raw = key.utf8_text(source).ok()?;
        if raw.trim_matches(|c| c == '"' || c == '\'') == needle {
            return child.child_by_field_name("value");
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(src, &Check, "foo.test.ts")
    }

    fn run_nontest(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(src, &Check, "foo.ts")
    }

    #[test]
    fn flags_bare_new_query_client_in_test() {
        assert_eq!(run_test("const c = new QueryClient();").len(), 1);
    }

    #[test]
    fn flags_missing_retry_false_in_test() {
        let src = "const c = new QueryClient({ defaultOptions: { queries: { staleTime: 0 } } });";
        assert_eq!(run_test(src).len(), 1);
    }

    #[test]
    fn allows_retry_false_in_test() {
        let src = "const c = new QueryClient({ defaultOptions: { queries: { retry: false } } });";
        assert!(run_test(src).is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        assert!(run_nontest("const c = new QueryClient();").is_empty());
    }
}
