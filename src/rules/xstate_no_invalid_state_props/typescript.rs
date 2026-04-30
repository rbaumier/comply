//! xstate-no-invalid-state-props — flag unknown properties inside XState
//! state node config objects.
//!
//! Heuristic: find a `pair` whose key is `states` and whose value is an
//! object literal. Each direct child `pair` of that object is treated as a
//! state node (e.g. `idle: { ... }`). Inside each state node object, every
//! direct `pair` key is validated against the whitelist of known XState
//! state node properties. Unknown keys are typically typos
//! (`entires` instead of `entry`) or misplaced config.

use crate::diagnostic::{Diagnostic, Severity};

/// Whitelist of valid XState state node properties.
const VALID_STATE_PROPS: &[&str] = &[
    "id",
    "initial",
    "type",
    "context",
    "states",
    "on",
    "entry",
    "exit",
    "invoke",
    "after",
    "always",
    "onDone",
    "meta",
    "tags",
    "description",
    "history",
    "target",
    "actions",
    "data",
    "output",
];

/// Strip matching quote characters from an identifier/string key text.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

/// Iterate direct `pair` children of an `object` node.
fn object_pairs<'a>(object: tree_sitter::Node<'a>) -> impl Iterator<Item = tree_sitter::Node<'a>> {
    let mut cursor = object.walk();
    let mut pairs = Vec::new();
    for child in object.children(&mut cursor) {
        if child.kind() == "pair" {
            pairs.push(child);
        }
    }
    pairs.into_iter()
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    // Anchor on `states: { ... }` pairs.
    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = unquote(key.utf8_text(source).unwrap_or(""));
    if key_text != "states" {
        return;
    }
    let Some(states_value) = node.child_by_field_name("value") else { return };
    if states_value.kind() != "object" {
        return;
    }

    // Each direct pair inside `states: { ... }` is a state node config.
    for state_pair in object_pairs(states_value) {
        let Some(state_value) = state_pair.child_by_field_name("value") else { continue };
        if state_value.kind() != "object" {
            continue;
        }

        // Each direct pair inside the state node object is a state prop.
        for prop_pair in object_pairs(state_value) {
            let Some(prop_key) = prop_pair.child_by_field_name("key") else { continue };
            let prop_text = unquote(prop_key.utf8_text(source).unwrap_or(""));
            if prop_text.is_empty() {
                continue;
            }
            if VALID_STATE_PROPS.contains(&prop_text) {
                continue;
            }

            let pos = prop_key.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{prop_text}` is not a valid XState state node property."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_unknown_prop_in_state_node() {
        let src = r#"
            createMachine({
                states: {
                    idle: {
                        entires: ["log"],
                    },
                },
            });
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("entires"));
    }

    #[test]
    fn flags_multiple_unknown_props() {
        let src = r#"
            createMachine({
                states: {
                    idle: {
                        entry: "log",
                        foobar: 1,
                        bazqux: 2,
                    },
                },
            });
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_all_valid_props() {
        let src = r#"
            createMachine({
                states: {
                    idle: {
                        id: "idle",
                        initial: "a",
                        type: "atomic",
                        context: {},
                        states: {},
                        on: {},
                        entry: "log",
                        exit: "cleanup",
                        invoke: { src: "x" },
                        after: {},
                        always: [],
                        onDone: "next",
                        meta: {},
                        tags: [],
                        description: "idle state",
                        history: "shallow",
                        target: "next",
                        actions: [],
                        data: {},
                        output: {},
                    },
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_unknown_in_nested_states() {
        let src = r#"
            createMachine({
                states: {
                    parent: {
                        initial: "child",
                        states: {
                            child: {
                                typo: true,
                            },
                        },
                    },
                },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_non_xstate_objects() {
        let src = r#"
            const config = {
                states: "california",
            };
            const obj = {
                things: { one: { whatever: 1 } },
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_state_value_that_is_not_object() {
        let src = r#"
            createMachine({
                states: {
                    idle: "atomic",
                    running: 42,
                },
            });
        "#;
        assert!(run(src).is_empty());
    }
}
