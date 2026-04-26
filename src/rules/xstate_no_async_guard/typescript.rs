//! xstate-no-async-guard backend — flag `guard` or `cond` object properties
//! whose value is an async arrow function or async function expression.
//!
//! XState guards must be synchronous predicates returning a boolean. Async
//! guards silently return a `Promise`, which is truthy and breaks transition
//! evaluation. Async logic belongs in actors (invoked services), not guards.

use crate::diagnostic::{Diagnostic, Severity};

/// True if `node` is an arrow function or function expression with the
/// `async` modifier.
fn is_async_function(node: tree_sitter::Node, source: &[u8]) -> bool {
    let kind = node.kind();
    if kind != "arrow_function" && kind != "function_expression" {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "async" {
            return true;
        }
    }
    // Fallback: some grammar variants expose `async` as the leading token
    // without a distinct node kind. Check the leading source text.
    node.utf8_text(source)
        .map(|t| t.trim_start().starts_with("async"))
        .unwrap_or(false)
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key_node) = node.child_by_field_name("key") else { return };
    let key_text = key_node.utf8_text(source).unwrap_or("");
    let key_text = key_text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    if key_text != "guard" && key_text != "cond" {
        return;
    }

    let Some(value_node) = node.child_by_field_name("value") else { return };
    if !is_async_function(value_node, source) {
        return;
    }

    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "xstate-no-async-guard".into(),
        message: format!(
            "`{key_text}` must be synchronous — async guards return a Promise (always truthy). Use an actor for async logic."
        ),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_async_arrow_guard() {
        let src = r#"
            const machine = createMachine({
                on: {
                    NEXT: {
                        target: 'b',
                        guard: async (ctx) => await isAllowed(ctx),
                    },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_async_function_expression_guard() {
        let src = r#"
            const machine = createMachine({
                on: {
                    NEXT: {
                        guard: async function (ctx) { return await check(ctx); },
                    },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_async_cond_legacy_api() {
        let src = r#"
            const machine = Machine({
                on: {
                    NEXT: {
                        cond: async (ctx) => await check(ctx),
                    },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_sync_arrow_guard() {
        let src = r#"
            const machine = createMachine({
                on: {
                    NEXT: { guard: (ctx) => ctx.count > 0 },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_sync_cond_guard() {
        let src = r#"
            const machine = createMachine({
                on: {
                    NEXT: { cond: function (ctx) { return ctx.count > 0; } },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_guard_reference_by_name() {
        let src = r#"
            const machine = createMachine({
                on: {
                    NEXT: { guard: 'isAllowed' },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_unrelated_async_property() {
        let src = r#"
            const config = {
                handler: async (req) => await fetch(req),
            };
        "#;
        assert!(run_on(src).is_empty());
    }
}
