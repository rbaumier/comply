//! tanstack-query-no-mutation-for-client-state backend.
//!
//! Heuristic: a `useMutation({ mutationFn: (...) => body })` whose body
//! contains no network call (no `fetch`, no `axios.*`, no `api.*`
//! method call, no `await` at all) is almost always misuse — the
//! developer is reaching for `useMutation` to paper over `useState`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.utf8_text(source).ok() != Some("useMutation") { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(options) = args.named_child(0) else { return; };
    if options.kind() != "object" { return; }

    let Some(mutation_fn) = find_pair_value(options, source, "mutationFn") else { return; };
    let body = match mutation_fn.kind() {
        "arrow_function" | "function_expression" | "function" => {
            match mutation_fn.child_by_field_name("body") {
                Some(b) => b,
                None => return,
            }
        }
        _ => return,
    };

    if body_has_network_call(body, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`useMutation` has no network call in its `mutationFn` — use `useState`/`useReducer` instead of abusing TanStack Query for local state.".into(),
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

fn body_has_network_call(body: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut found = false;
    walk_subtree(body, &mut |n| {
        if found { return; }
        if n.kind() == "await_expression" { found = true; return; }
        if n.kind() != "call_expression" { return; }
        let Some(func) = n.child_by_field_name("function") else { return; };
        let Ok(text) = func.utf8_text(source) else { return; };
        if text == "fetch" { found = true; return; }
        // `axios(...)`, `axios.post(...)`, `api.post(...)`, `api.foo.bar(...)`.
        if text.starts_with("axios") || text.starts_with("api.") || text == "api" {
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
    fn flags_pure_local_mutation() {
        let src = "useMutation({ mutationFn: (x) => x + 1 });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_mutation_setting_local_state() {
        let src = "useMutation({ mutationFn: (v) => { setCount(v); return v; } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_mutation_with_fetch() {
        let src = "useMutation({ mutationFn: (t) => fetch('/t', { method: 'POST', body: t }) });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_with_axios() {
        let src = "useMutation({ mutationFn: (t) => axios.post('/t', t) });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_with_api_call() {
        let src = "useMutation({ mutationFn: (t) => api.createTodo(t) });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_with_await() {
        let src = "useMutation({ mutationFn: async (t) => { await doWork(t); } });";
        assert!(run(src).is_empty());
    }
}
