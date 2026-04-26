//! tanstack-query-no-mutation-for-client-state backend.
//!
//! Heuristic: a `useMutation({ mutationFn: (...) => body })` whose body
//! contains no concrete HTTP call is almost always misuse — the
//! developer is reaching for `useMutation` to paper over `useState`.
//!
//! "Concrete HTTP call" = `fetch(...)`, `axios(...)` / `axios.<verb>(...)`,
//! `api(...)` / `api.<...>(...)`, or any `.get/.post/.put/.patch/.delete(...)`
//! method call. A bare `await someLocalFn()` is NOT considered network
//! activity — it could be doing anything (timer, IndexedDB, channel
//! send) and would fall into the same misuse pattern.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
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

const HTTP_VERB_METHODS: &[&str] = &["get", "post", "put", "patch", "delete"];

fn body_has_network_call(body: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut found = false;
    walk_subtree(body, &mut |n| {
        if found { return; }
        if n.kind() != "call_expression" { return; }
        let Some(func) = n.child_by_field_name("function") else { return; };
        // Bare callee: `fetch(...)`, `axios(...)`, `api(...)`.
        if func.kind() == "identifier" {
            let Ok(name) = func.utf8_text(source) else { return; };
            if matches!(name, "fetch" | "axios" | "api") {
                found = true;
            }
            return;
        }
        // Member callee: `axios.post(...)`, `api.foo.bar(...)`,
        // `<x>.get/.post/.put/.patch/.delete(...)`.
        if func.kind() == "member_expression" {
            let Some(prop) = func.child_by_field_name("property") else { return; };
            let Ok(method) = prop.utf8_text(source) else { return; };
            if HTTP_VERB_METHODS.contains(&method) {
                found = true;
                return;
            }
            // Match the `axios` / `api` chain even when the verb name is unusual.
            let Ok(full) = func.utf8_text(source) else { return; };
            if full.starts_with("axios.") || full.starts_with("api.") {
                found = true;
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
    fn flags_mutation_with_bare_await_no_http_call() {
        // REVIEW regression: `await doWork(t)` could be local
        // (Promise.resolve, IndexedDB, channel send). It is NOT a
        // network call by itself and must not exempt the mutation.
        let src = "useMutation({ mutationFn: async (t) => { await doWork(t); } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_mutation_with_http_verb_method() {
        let src = "useMutation({ mutationFn: (t) => http.put('/t', t) });";
        assert!(run(src).is_empty());
    }
}
