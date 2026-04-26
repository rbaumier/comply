//! tanstack-query-no-global-onerror-v5 backend.
//!
//! Flags `new QueryClient({ defaultOptions: { queries: { onError } } })`.
//! In v5 the per-observer `onError` callback on queries was removed —
//! global error handling belongs on the `QueryCache`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    let Some(constructor) = node.child_by_field_name("constructor") else { return; };
    if constructor.utf8_text(source).ok() != Some("QueryClient") { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(opts) = args.named_child(0) else { return; };
    if opts.kind() != "object" { return; }

    let Some(default_options) = find_pair_value(opts, source, "defaultOptions") else { return; };
    if default_options.kind() != "object" { return; }
    let Some(queries) = find_pair_value(default_options, source, "queries") else { return; };
    if queries.kind() != "object" { return; }
    let Some(on_error_pair) = find_pair(queries, source, "onError") else { return; };

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &on_error_pair,
        super::META.id,
        "`defaultOptions.queries.onError` was removed in v5. Handle global errors via `new QueryCache({ onError })`.".into(),
        Severity::Error,
    ));
}

fn find_pair<'a>(
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
            return Some(child);
        }
    }
    None
}

fn find_pair_value<'a>(
    object: tree_sitter::Node<'a>,
    source: &[u8],
    needle: &str,
) -> Option<tree_sitter::Node<'a>> {
    find_pair(object, source, needle).and_then(|p| p.child_by_field_name("value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_on_error_in_default_queries() {
        let src = "new QueryClient({ defaultOptions: { queries: { onError: handle } } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_on_error_with_arrow() {
        let src = "new QueryClient({ defaultOptions: { queries: { onError: (e) => log(e) } } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_query_cache_on_error() {
        let src = "new QueryClient({ queryCache: new QueryCache({ onError: handle }) });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_default_options() {
        let src = "new QueryClient({});";
        assert!(run(src).is_empty());
    }
}
