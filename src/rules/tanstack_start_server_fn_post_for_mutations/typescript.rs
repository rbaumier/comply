//! Flag `const fooUser = createServerFn({ ... })` where the binding name
//! starts with a mutation verb (create/update/delete/login/logout) and the
//! config object does not set `method: 'POST'`.

use crate::diagnostic::{Diagnostic, Severity};

const MUTATION_PREFIXES: &[&str] = &["create", "update", "delete", "login", "logout"];

crate::ast_check! { on ["variable_declarator"] prefilter = ["createServerFn"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let Ok(name) = name_node.utf8_text(source) else { return; };
    let lower = name.to_ascii_lowercase();
    if !MUTATION_PREFIXES.iter().any(|p| lower.starts_with(p)) { return; }

    let Some(value) = node.child_by_field_name("value") else { return; };
    let Some(call) = find_create_server_fn_call(value, source) else { return; };
    let Some(args) = call.child_by_field_name("arguments") else { return; };
    let options = first_object_argument(args);
    let has_post = options
        .and_then(|o| find_pair_value(o, source, "method"))
        .and_then(|n| n.utf8_text(source).ok())
        .map(|v| v.trim_matches(|c| c == '"' || c == '\'').eq_ignore_ascii_case("POST"))
        .unwrap_or(false);
    if has_post { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &call,
        super::META.id,
        format!(
            "Server function `{name}` is named like a mutation but does not \
             declare `method: 'POST'`. Mutations should not be GET-accessible."
        ),
        Severity::Warning,
    ));
}

/// Walks a declarator's value to find a `createServerFn(...)` call.
/// Handles both `createServerFn({...})` and chained `createServerFn({...}).handler(...)`.
fn find_create_server_fn_call<'a>(
    value: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut stack = vec![value];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(callee) = n.child_by_field_name("function")
                && let Ok(text) = callee.utf8_text(source)
                    && (text == "createServerFn" || text.ends_with(".createServerFn")) {
                        return Some(n);
                    }
        let mut cursor = n.walk();
        for c in n.children(&mut cursor) {
            stack.push(c);
        }
    }
    None
}

fn first_object_argument<'a>(args: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = args.walk();
    args.children(&mut cursor).find(|c| c.kind() == "object")
}

fn find_pair_value<'a>(
    object: tree_sitter::Node<'a>,
    source: &[u8],
    key: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = object.walk();
    for child in object.children(&mut cursor) {
        if child.kind() != "pair" { continue; }
        let Some(k) = child.child_by_field_name("key") else { continue; };
        let Ok(raw) = k.utf8_text(source) else { continue; };
        let name = raw.trim_matches(|c| c == '"' || c == '\'');
        if name == key {
            return child.child_by_field_name("value");
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_create_without_post() {
        assert_eq!(
            run("const createUser = createServerFn({}).handler(fn);").len(),
            1
        );
    }

    #[test]
    fn flags_delete_with_get() {
        assert_eq!(
            run("const deletePost = createServerFn({ method: 'GET' }).handler(fn);").len(),
            1
        );
    }

    #[test]
    fn allows_create_with_post() {
        assert!(
            run("const createUser = createServerFn({ method: 'POST' }).handler(fn);").is_empty()
        );
    }

    #[test]
    fn allows_getter_name() {
        assert!(
            run("const getUser = createServerFn({ method: 'GET' }).handler(fn);").is_empty()
        );
    }
}
