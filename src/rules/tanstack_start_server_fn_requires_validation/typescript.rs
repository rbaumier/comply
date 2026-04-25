//! tanstack-start-server-fn-requires-validation backend — flag every
//! `createServerFn(...)` call expression in a file that does not also
//! invoke `.input(...)`, `.safeParse(...)`, or `.parse(...)` somewhere.

use crate::diagnostic::{Diagnostic, Severity};

const VALIDATION_METHODS: &[&str] = &["input", "safeParse", "parse"];

/// Return true if `node` is a call to a method named `input`, `safeParse`,
/// or `parse`.
fn is_validation_call(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
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
    VALIDATION_METHODS.iter().any(|n| *n == name)
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

        let has_validation = calls.iter().any(|n| is_validation_call(*n, source));
        if has_validation {
            return Vec::new();
        }

        server_fn_calls
            .into_iter()
            .map(|node| {
                Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    "`createServerFn` without `.input()` validation accepts unvalidated data at the RPC boundary.".into(),
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
    fn flags_no_input_validation() {
        assert_eq!(
            run("const fn = createServerFn().handler(async () => { await db.delete(x) })").len(),
            1
        );
    }

    #[test]
    fn allows_with_input() {
        assert!(run(
            "const fn = createServerFn().input(z.object({ id: z.string() })).handler(async (ctx) => {})"
        )
        .is_empty());
    }

    #[test]
    fn ignores_non_server_fn_files() {
        assert!(run("const x = 1;").is_empty());
    }
}
