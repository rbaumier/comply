//! tanstack-start-server-fn-requires-validation backend — flag every
//! `createServerFn(...)` call expression in a file that does not also
//! invoke `.input(...)`, `.safeParse(...)`, or `.parse(...)` somewhere,
//! unless the handler callback accepts no parameters (no caller input).

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

/// Walk a method-chained call expression to find the innermost `createServerFn()` node.
fn find_create_server_fn_node<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    if node.kind() != "call_expression" {
        return None;
    }
    let function = node.child_by_field_name("function")?;
    if function.kind() == "identifier" && function.utf8_text(source).ok() == Some("createServerFn")
    {
        return Some(node);
    }
    if function.kind() == "member_expression" {
        let object = function.child_by_field_name("object")?;
        return find_create_server_fn_node(object, source);
    }
    None
}

/// Return the start byte of the `createServerFn()` call that this `.handler()` belongs to,
/// if and only if the handler callback accepts no parameters (no caller input).
fn no_input_server_fn_start(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<usize> {
    if node.kind() != "call_expression" {
        return None;
    }
    let function = node.child_by_field_name("function")?;
    if function.kind() != "member_expression" {
        return None;
    }
    let prop = function.child_by_field_name("property")?;
    if prop.utf8_text(source).ok() != Some("handler") {
        return None;
    }
    let args = node.child_by_field_name("arguments")?;
    // Find the first function-like argument.
    for i in 0..args.named_child_count() {
        let arg = args.named_child(i)?;
        if matches!(arg.kind(), "arrow_function" | "function") {
            let params = arg.child_by_field_name("parameters")?;
            if params.named_child_count() > 0 {
                // Handler takes parameters — caller supplies input, validation required.
                return None;
            }
            // No parameters — no caller input.
            let object = function.child_by_field_name("object")?;
            return find_create_server_fn_node(object, source).map(|n| n.start_byte());
        }
    }
    None
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

        // Collect start bytes of createServerFn() calls whose handler takes no params.
        let no_input_starts: Vec<usize> = calls
            .iter()
            .copied()
            .filter_map(|n| no_input_server_fn_start(n, source))
            .collect();

        let server_fn_calls_needing_validation: Vec<_> = server_fn_calls
            .iter()
            .copied()
            .filter(|n| !no_input_starts.contains(&n.start_byte()))
            .collect();

        if server_fn_calls_needing_validation.is_empty() {
            return Vec::new();
        }

        let has_validation = calls.iter().any(|n| is_validation_call(*n, source));
        if has_validation {
            return Vec::new();
        }

        server_fn_calls_needing_validation
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
            run("const fn = createServerFn().handler(async ({ id }) => { await db.delete(id) })").len(),
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

    #[test]
    fn no_fp_handler_with_no_params() {
        // Regression for #484 — server function that reads from request headers
        // has no caller-supplied input; requiring .input() here is meaningless.
        assert!(run(
            "const getSessionSsr = createServerFn().handler(async () => { return getSessionFromHeaders(auth, getRequest().headers); })"
        )
        .is_empty());
    }

    #[test]
    fn flags_handler_with_params_no_validation() {
        // A handler that receives data must still be validated.
        assert_eq!(
            run("const fn = createServerFn().handler(async ({ data }) => { await db.insert(data) })").len(),
            1
        );
    }

    #[test]
    fn no_fp_async_no_params_handler() {
        // Async arrow function with no params should not trigger the rule.
        assert!(run(
            "const fn = createServerFn().handler(async () => { return fetchData(); })"
        )
        .is_empty());
    }
}
