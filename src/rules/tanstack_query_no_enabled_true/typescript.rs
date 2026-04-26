//! tanstack-query-no-enabled-true backend.
//!
//! Flag `enabled: true` literal pairs inside query hook calls. The default
//! is `true`, so spelling it out is noise that signals confusion about the
//! option's behavior.

use crate::diagnostic::{Diagnostic, Severity};

const HOOKS: &[&str] = &[
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
    "queryOptions",
];

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return; };
    let Ok(key_text) = key.utf8_text(source) else { return; };
    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key_name != "enabled" { return; }
    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "true" { return; }
    if !inside_query_hook(node, source) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`enabled: true` is redundant — queries are enabled by default.".into(),
        Severity::Warning,
    ));
}

fn inside_query_hook(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "call_expression"
            && let Some(func) = parent.child_by_field_name("function")
            && let Ok(name) = func.utf8_text(source)
            && HOOKS.contains(&name)
        {
            return true;
        }
        current = parent;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_enabled_true() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], queryFn: f, enabled: true })").len(),
            1
        );
    }

    #[test]
    fn allows_enabled_condition() {
        assert!(run("useQuery({ queryKey: ['x'], queryFn: f, enabled: !!userId })").is_empty());
    }

    #[test]
    fn allows_no_enabled() {
        assert!(run("useQuery({ queryKey: ['x'], queryFn: f })").is_empty());
    }
}
