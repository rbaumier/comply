//! tanstack-query-invalidate-after-mutation backend.
//!
//! Detects a `useMutation({ mutationFn: … })` whose `mutationFn`
//! performs a write (POST/PUT/PATCH/DELETE via fetch) but whose options
//! object does not include an `onSuccess` or `onSettled` callback
//! containing `invalidateQueries` / `setQueryData`. Skipping the
//! invalidation leaves stale data in dependent `useQuery` caches.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["mutationFn"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.utf8_text(source).ok() != Some("useMutation") { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(options) = args.named_child(0) else { return; };
    if options.kind() != "object" { return; }

    let Some(mutation_fn) = find_pair_value(options, source, "mutationFn") else { return; };
    if !is_write_mutation(mutation_fn, source) { return; }

    let has_handler = ["onSuccess", "onSettled"].iter().any(|name| {
        find_pair_value(options, source, name)
            .map(|v| handler_calls_cache_update(v, source))
            .unwrap_or(false)
    });
    if has_handler { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`useMutation` performs a write but does not update the query cache. \
         Add `onSuccess` or `onSettled` that calls `invalidateQueries` or `setQueryData`.".into(),
        Severity::Warning,
    ));
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

/// True when the node is an arrow/function whose body calls `fetch`
/// with a method of POST/PUT/PATCH/DELETE. A missing method or a GET
/// doesn't count.
fn is_write_mutation(value: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let body = match value.kind() {
        "arrow_function" | "function_expression" | "function" => {
            match value.child_by_field_name("body") {
                Some(b) => b,
                None => return false,
            }
        }
        _ => return false,
    };

    let mut found = false;
    walk_subtree(body, &mut |n| {
        if found { return; }
        if n.kind() != "call_expression" { return; }
        let Some(func) = n.child_by_field_name("function") else { return; };
        if func.utf8_text(source).ok() != Some("fetch") { return; }
        let Some(args) = n.child_by_field_name("arguments") else { return; };
        let Some(opts) = args.named_child(1) else { return; };
        if opts.kind() != "object" { return; }
        let Some(method) = find_pair_value(opts, source, "method") else { return; };
        let Ok(text) = method.utf8_text(source) else { return; };
        let trimmed = text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
        if matches!(trimmed.to_ascii_uppercase().as_str(), "POST" | "PUT" | "PATCH" | "DELETE") {
            found = true;
        }
    });
    found
}

fn handler_calls_cache_update(handler: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let body = match handler.kind() {
        "arrow_function" | "function_expression" | "function" => {
            match handler.child_by_field_name("body") {
                Some(b) => b,
                None => return false,
            }
        }
        _ => return false,
    };

    let mut found = false;
    walk_subtree(body, &mut |n| {
        if found { return; }
        if n.kind() != "call_expression" { return; }
        let Some(func) = n.child_by_field_name("function") else { return; };
        let Ok(text) = func.utf8_text(source) else { return; };
        if text.ends_with("invalidateQueries")
            || text.ends_with("setQueryData")
            || text.ends_with("refetchQueries")
            || text.ends_with("removeQueries")
        {
            found = true;
        }
    });
    found
}

fn walk_subtree<F: FnMut(tree_sitter::Node<'_>)>(root: tree_sitter::Node<'_>, visit: &mut F) {
    let root_id = root.id();
    let mut cursor = root.walk();
    loop {
        visit(cursor.node());
        if cursor.goto_first_child() { continue; }
        loop {
            if cursor.node().id() == root_id { return; }
            if cursor.goto_next_sibling() { break; }
            if !cursor.goto_parent() { return; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_post_without_on_success() {
        let src = "useMutation({ mutationFn: (t) => fetch('/t', { method: 'POST', body: t }) });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_without_handlers() {
        let src = "useMutation({ mutationFn: (id) => fetch('/t/' + id, { method: 'DELETE' }) });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_post_with_invalidate_queries() {
        let src = "useMutation({ mutationFn: (t) => fetch('/t', { method: 'POST' }), onSuccess: () => qc.invalidateQueries({ queryKey: ['t'] }) });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_post_with_set_query_data() {
        let src = "useMutation({ mutationFn: (t) => fetch('/t', { method: 'POST' }), onSettled: () => qc.setQueryData(['t'], []) });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_get_mutation() {
        let src = "useMutation({ mutationFn: () => fetch('/t', { method: 'GET' }) });";
        assert!(run(src).is_empty());
    }
}
