//! xstate-no-imperative-action backend — flag `send(...)` or `raise(...)`
//! called outside of an action context.
//!
//! In XState, `send` and `raise` are action creators. They must be used as
//! action values (e.g. `actions: [send({ type: 'NEXT' })]`) or returned
//! from an action function — not invoked imperatively at top level, inside
//! guards, or inside service factory bodies, because the call result is an
//! action descriptor, not a side-effect.
//!
//! Heuristic for "action context":
//! - The call is (transitively) the value of an object pair whose key is
//!   `actions`, `entry`, or `exit`.
//! - OR the call sits inside an arrow/function body that is itself the
//!   value of such a pair.
//!
//! Anything else — top-level `send(...)`, `raise(...)` in a guard body,
//! etc. — is flagged.

use crate::diagnostic::{Diagnostic, Severity};

const IMPERATIVE_ACTION_KEYS: &[&str] = &["actions", "entry", "exit"];

/// Strip matching surrounding quote characters from a property key.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

/// True if `node` appears inside an action-valued object pair (`actions`,
/// `entry`, or `exit`). We walk ancestors looking for a `pair` whose key
/// matches — this covers `actions: send(...)`, `actions: [send(...)]`,
/// `entry: () => send(...)`, etc.
fn is_inside_action_context(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "pair"
            && let Some(key) = n.child_by_field_name("key")
        {
            let key_text = unquote(key.utf8_text(source).unwrap_or(""));
            if IMPERATIVE_ACTION_KEYS.contains(&key_text) {
                return true;
            }
        }
        cur = n.parent();
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    let name = callee.utf8_text(source).unwrap_or("");
    if name != "send" && name != "raise" {
        return;
    }

    if is_inside_action_context(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`{name}(...)` must be called inside an action (e.g. `actions: [{name}(...)]`), not imperatively."
        ),
        severity: Severity::Warning,
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
    fn allows_send_inside_actions_array() {
        let src = r#"
            createMachine({
                on: {
                    NEXT: {
                        actions: [send({ type: 'GO' })],
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_raise_inside_entry() {
        let src = r#"
            createMachine({
                states: {
                    idle: {
                        entry: raise({ type: 'START' }),
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_send_inside_entry_arrow() {
        let src = r#"
            createMachine({
                states: {
                    idle: {
                        entry: () => send({ type: 'GO' }),
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_top_level_send() {
        let src = "send({ type: 'GO' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_raise_inside_guard() {
        let src = r#"
            createMachine({
                on: {
                    NEXT: {
                        guard: () => raise({ type: 'X' }),
                    },
                },
            });
        "#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("raise"));
    }

    #[test]
    fn ignores_unrelated_functions() {
        let src = "sendEmail({ to: 'a' }); raiseHell();";
        assert!(run_on(src).is_empty());
    }
}
