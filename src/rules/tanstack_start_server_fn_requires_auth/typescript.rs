//! tanstack-start-server-fn-requires-auth backend — flag a file that
//! uses `createServerFn` AND performs `.insert(...)`/`.update(...)`/
//! `.delete(...)` mutations without invoking any auth helper.

use crate::diagnostic::{Diagnostic, Severity};

const AUTH_CALLEES: &[&str] = &[
    "getSession",
    "auth",
    "verifySession",
    "requireAuth",
    "currentUser",
];

const MUTATION_METHODS: &[&str] = &["insert", "update", "delete"];

/// Return true if `node` is a call to one of the auth-helper names.
fn is_auth_call(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    match function.kind() {
        "identifier" => {
            let Ok(name) = function.utf8_text(source) else {
                return false;
            };
            AUTH_CALLEES.iter().any(|n| *n == name)
        }
        "member_expression" => {
            let Some(prop) = function.child_by_field_name("property") else {
                return false;
            };
            let Ok(name) = prop.utf8_text(source) else {
                return false;
            };
            AUTH_CALLEES.iter().any(|n| *n == name)
        }
        _ => false,
    }
}

/// Return true if `node` is a `.insert(...)`, `.update(...)`, or
/// `.delete(...)` method call.
fn is_mutation_call(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = function.child_by_field_name("property") else {
        return false;
    };
    let Ok(name) = prop.utf8_text(source) else {
        return false;
    };
    MUTATION_METHODS.iter().any(|n| *n == name)
}

/// Return true if `node` is a call to `createServerFn` (bare identifier).
fn is_create_server_fn(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    function.kind() == "identifier" && function.utf8_text(source).ok() == Some("createServerFn")
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
        let calls = crate::rules::walker::collect_nodes_of_kinds(tree, &["call_expression"]);

        let server_fn_calls: Vec<_> = calls
            .iter()
            .copied()
            .filter(|n| is_create_server_fn(*n, source))
            .collect();
        if server_fn_calls.is_empty() {
            return Vec::new();
        }

        let has_mutation = calls.iter().any(|n| is_mutation_call(*n, source));
        if !has_mutation {
            return Vec::new();
        }

        let has_auth = calls.iter().any(|n| is_auth_call(*n, source));
        if has_auth {
            return Vec::new();
        }

        server_fn_calls
            .into_iter()
            .map(|node| {
                Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    "`createServerFn` with mutations must verify authentication before proceeding."
                        .into(),
                    Severity::Warning,
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(s, &Check, "api.functions.ts")
    }

    #[test]
    fn flags_mutation_without_auth() {
        assert_eq!(
            run("const del = createServerFn().handler(async () => { await db.delete(posts) })")
                .len(),
            1
        );
    }

    #[test]
    fn allows_with_get_session() {
        assert!(run(
            "const del = createServerFn().handler(async () => { const s = await getSession(); await db.delete(posts) })"
        )
        .is_empty());
    }

    #[test]
    fn allows_read_only() {
        assert!(
            run("const get = createServerFn().handler(async () => db.select().from(posts))")
                .is_empty()
        );
    }
}
