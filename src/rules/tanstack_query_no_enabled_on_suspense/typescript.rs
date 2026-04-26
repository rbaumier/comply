//! tanstack-query-no-enabled-on-suspense backend.
//!
//! Flags `useSuspenseQuery({ ..., enabled: ... })` and
//! `useSuspenseInfiniteQuery({ ..., enabled: ... })`. The suspense
//! variants always fetch — there is no "skipped" state that could
//! reasonably be represented during suspension.

use crate::diagnostic::{Diagnostic, Severity};

const SUSPENSE_HOOKS: &[&str] = &["useSuspenseQuery", "useSuspenseInfiniteQuery"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    let Ok(func_text) = func.utf8_text(source) else { return; };
    if !SUSPENSE_HOOKS.contains(&func_text) { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(options) = args.named_child(0) else { return; };
    if options.kind() != "object" { return; }

    let mut cursor = options.walk();
    for child in options.named_children(&mut cursor) {
        if child.kind() != "pair" { continue; }
        let Some(key) = child.child_by_field_name("key") else { continue; };
        let Ok(raw) = key.utf8_text(source) else { continue; };
        if raw.trim_matches(|c| c == '"' || c == '\'') == "enabled" {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &child,
                super::META.id,
                format!("`{func_text}` does not accept `enabled`. Conditionally render the component instead."),
                Severity::Error,
            ));
            break;
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
    fn flags_enabled_on_suspense_query() {
        let src = "useSuspenseQuery({ queryKey: ['x'], queryFn: f, enabled: !!id });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_enabled_on_suspense_infinite_query() {
        let src = "useSuspenseInfiniteQuery({ queryKey: ['x'], queryFn: f, initialPageParam: 0, enabled: false });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_suspense_without_enabled() {
        let src = "useSuspenseQuery({ queryKey: ['x'], queryFn: f });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_enabled_on_regular_use_query() {
        let src = "useQuery({ queryKey: ['x'], queryFn: f, enabled: !!id });";
        assert!(run(src).is_empty());
    }
}
