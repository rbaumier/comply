//! tanstack-query-object-syntax backend.
//!
//! Detects calls to `useQuery`, `useMutation`, `useInfiniteQuery`,
//! `useSuspenseQuery`, or `useSuspenseInfiniteQuery` where the first
//! argument is not an object literal. In TanStack Query v5 the
//! positional form `useQuery(key, fn, opts)` was removed — only the
//! single-object form is supported.

use crate::diagnostic::{Diagnostic, Severity};

const HOOKS: &[&str] = &[
    "useQuery",
    "useMutation",
    "useInfiniteQuery",
    "useSuspenseQuery",
    "useSuspenseInfiniteQuery",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    let Ok(func_text) = func.utf8_text(source) else { return; };
    if !HOOKS.contains(&func_text) { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(first) = args.named_child(0) else { return; };
    if matches!(first.kind(), "object" | "identifier" | "call_expression") {
        // `object` is the correct form. `identifier` / `call_expression`
        // almost certainly yield an options object (factory, variable) —
        // we can't know statically, so we allow it to avoid FPs.
        if first.kind() == "object" { return; }
        return;
    }
    // Any other shape — string literal, array, template, number — is a
    // positional legacy call and not valid in v5.
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{func_text}` must be called with an options object: \
             `{func_text}({{ queryKey, queryFn }})`. The positional form was removed in v5."
        ),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_positional_use_query() {
        assert_eq!(run("useQuery(['todos'], fetchTodos);").len(), 1);
    }

    #[test]
    fn flags_positional_use_mutation_with_string_key() {
        assert_eq!(run("useMutation('todos', createTodo);").len(), 1);
    }

    #[test]
    fn allows_object_syntax() {
        assert!(run("useQuery({ queryKey: ['todos'], queryFn: f });").is_empty());
    }

    #[test]
    fn allows_mutation_object_syntax() {
        assert!(run("useMutation({ mutationFn: f });").is_empty());
    }
}
