//! xstate-no-invalid-transition-props backend — flag unknown keys inside
//! XState transition objects nested under an `on: { EVENT: { ... } }` handler.
//!
//! XState transition objects accept a fixed set of properties. Typos such as
//! `taget` instead of `target`, or legacy-but-removed keys like `in`, silently
//! produce malformed transitions that XState accepts without warning. This
//! rule walks every `pair` whose enclosing object is the value of an `on`
//! entry's per-event object, and flags any key outside the whitelist.

use crate::diagnostic::{Diagnostic, Severity};

/// Keys permitted on a transition object (`on: { EVENT: { ... } }`).
const VALID_TRANSITION_PROPS: &[&str] = &[
    "target",
    "guard",
    "cond",
    "actions",
    "internal",
    "description",
    "meta",
    "reenter",
];

/// Strip matching surrounding quote characters from a property key.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

/// True if `obj` is an `object` node that is the direct value of a `pair`
/// whose parent `object` is the value of a `pair` keyed `on`. In other words,
/// `obj` is a per-event transition object inside an `on: { EVENT: obj }` map.
fn is_transition_object(obj: tree_sitter::Node, source: &[u8]) -> bool {
    if obj.kind() != "object" {
        return false;
    }
    // The per-event `pair` (key = EVENT name, value = this object).
    let Some(event_pair) = obj.parent() else { return false };
    if event_pair.kind() != "pair" {
        return false;
    }
    // The enclosing `on` object.
    let Some(on_object) = event_pair.parent() else { return false };
    if on_object.kind() != "object" {
        return false;
    }
    // The `on: { ... }` pair itself.
    let Some(on_pair) = on_object.parent() else { return false };
    if on_pair.kind() != "pair" {
        return false;
    }
    let Some(on_key) = on_pair.child_by_field_name("key") else { return false };
    let key_text = on_key.utf8_text(source).unwrap_or("");
    unquote(key_text) == "on"
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    // The pair must live directly inside a transition object.
    let Some(parent_object) = node.parent() else { return };
    if !is_transition_object(parent_object, source) {
        return;
    }

    let Some(key_node) = node.child_by_field_name("key") else { return };
    let key_text = unquote(key_node.utf8_text(source).unwrap_or(""));
    if key_text.is_empty() {
        return;
    }
    if VALID_TRANSITION_PROPS.contains(&key_text) {
        return;
    }

    let pos = key_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`{key_text}` is not a valid XState transition property (allowed: {}).",
            VALID_TRANSITION_PROPS.join(", ")
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
    fn flags_typo_target() {
        let src = r#"
            createMachine({
                on: {
                    NEXT: {
                        taget: 'b',
                        actions: 'doIt',
                    },
                },
            });
        "#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("taget"));
    }

    #[test]
    fn flags_multiple_unknown_props() {
        let src = r#"
            createMachine({
                on: {
                    NEXT: {
                        target: 'b',
                        unknown1: 1,
                        unknown2: 2,
                    },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_all_valid_transition_props() {
        let src = r#"
            createMachine({
                on: {
                    NEXT: {
                        target: 'b',
                        guard: 'isReady',
                        cond: 'legacyGuard',
                        actions: ['log'],
                        internal: true,
                        description: 'go to b',
                        meta: { foo: 1 },
                        reenter: false,
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_keys_in_unrelated_objects() {
        let src = r#"
            const config = {
                something: {
                    taget: 'b',
                    whatever: true,
                },
            };
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_state_node_keys_outside_on() {
        // Keys like `entry`, `exit`, `invoke` are valid on state nodes but
        // NOT on transition objects. This rule only inspects transitions,
        // so state-node keys must not produce diagnostics.
        let src = r#"
            createMachine({
                states: {
                    idle: {
                        entry: 'log',
                        exit: 'cleanup',
                        invoke: { src: 'fetcher' },
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_unknown_prop_with_quoted_key() {
        let src = r#"
            createMachine({
                on: {
                    NEXT: {
                        'target': 'b',
                        'bogus': 1,
                    },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }
}
