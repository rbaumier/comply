//! tanstack-query-dependent-needs-enabled backend.
//!
//! Heuristic: when a `queryFn` body uses optional chaining (`x?.y`) or a
//! non-null assertion on an identifier (`x!`), we assume the query
//! depends on a possibly-undefined value. In that case the options
//! object must also carry an `enabled` key — otherwise the query fires
//! with `undefined` and either errors or caches a bogus key.
//!
//! Limitation: we cannot detect dependencies that are visible only to a
//! type-checker — e.g. `queryFn: () => fetchUser(user.id)` where
//! `user: User | undefined` in the surrounding scope. Tree-sitter has
//! no type information, so we deliberately stop at syntactic markers
//! (`?.` and `!`) and do not guess further. The `documents_type_info_limitation`
//! test below records this as a known false negative.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["queryFn"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    let Ok(func_text) = func.utf8_text(source) else { return; };
    if !matches!(func_text, "useQuery" | "useInfiniteQuery" | "queryOptions") { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(options) = args.named_child(0) else { return; };
    if options.kind() != "object" { return; }

    let Some(query_fn) = find_pair_value(options, source, "queryFn") else { return; };
    if !body_looks_dependent(query_fn, source) { return; }

    if has_key(options, source, "enabled") { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{func_text}` depends on a possibly-undefined value (optional chain or `!` assertion in queryFn) but has no `enabled`. \
             Add `enabled: !!dependency` to gate the request."
        ),
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

fn has_key(object: tree_sitter::Node<'_>, source: &[u8], needle: &str) -> bool {
    find_pair_value(object, source, needle).is_some()
}

fn body_looks_dependent(query_fn: tree_sitter::Node<'_>, _source: &[u8]) -> bool {
    // Only the arrow/function body — skip other shapes (identifier, call).
    let body = match query_fn.kind() {
        "arrow_function" | "function_expression" | "function" => {
            match query_fn.child_by_field_name("body") {
                Some(b) => b,
                None => return false,
            }
        }
        _ => return false,
    };

    let mut found = false;
    walk_subtree(body, &mut |n| {
        if found { return; }
        match n.kind() {
            "optional_chain" => { found = true; }
            "non_null_expression" => { found = true; }
            _ => {}
        }
        // tree-sitter-typescript exposes `a?.b` as a `member_expression`
        // where one of the children's node kind is the token `?.`. Catch
        // that form by checking for an `optional_chain` child token.
        if n.kind() == "member_expression" {
            let mut c = n.walk();
            for ch in n.children(&mut c) {
                if ch.kind() == "optional_chain" { found = true; break; }
            }
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
    fn flags_optional_chain_without_enabled() {
        let src = "useQuery({ queryKey: ['u', user?.id], queryFn: () => fetch('/u/' + user?.id) });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_non_null_assertion_without_enabled() {
        let src = "useQuery({ queryKey: ['u'], queryFn: () => fetchUser(user!.id) });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_optional_chain_with_enabled() {
        let src = "useQuery({ queryKey: ['u'], queryFn: () => fetch('/u/' + user?.id), enabled: !!user });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_dependent_query() {
        let src = "useQuery({ queryKey: ['u'], queryFn: () => fetch('/u') });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn documents_type_info_limitation() {
        // REVIEW: this rule is intentionally syntactic. A dependency
        // visible only to a TypeScript type-checker (e.g. `user: User |
        // undefined` referenced as `user.id` without `?.` / `!`) is a
        // known false negative. Expanding the heuristic would require
        // type info, which tree-sitter does not provide. We assert the
        // current behaviour so any future change is intentional.
        let src = "useQuery({ queryKey: ['u'], queryFn: () => fetchUser(user.id) });";
        assert!(run(src).is_empty());
    }
}
