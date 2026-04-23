//! xstate-no-misplaced-on-transition backend — flag `on` properties that live
//! where XState ignores them: inside an `invoke` configuration object, or
//! directly under a `states` map (instead of inside a state node).
//!
//! XState's `on` transitions are only honoured when attached to a state node.
//! Placing `on` inside `invoke` (alongside `src`, `id`, `onDone`) or directly
//! as a sibling of state-node keys under `states` silently drops the
//! transitions at runtime.

use crate::diagnostic::{Diagnostic, Severity};

/// Strip surrounding quotes from a tree-sitter key text.
fn clean_key(text: &str) -> &str {
    text.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

/// Return the key text of the `pair` whose `value` is `object`, if any.
///
/// Tree-sitter shape for `{ foo: { ... } }`:
///   pair(key=foo, value=object{...})
fn enclosing_pair_key<'a>(object: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let parent = object.parent()?;
    if parent.kind() != "pair" {
        return None;
    }
    let value = parent.child_by_field_name("value")?;
    if value.id() != object.id() {
        return None;
    }
    let key = parent.child_by_field_name("key")?;
    let text = key.utf8_text(source).ok()?;
    Some(clean_key(text))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "pair" {
        return;
    }

    let Some(key_node) = node.child_by_field_name("key") else { return };
    let key_text = key_node.utf8_text(source).unwrap_or("");
    if clean_key(key_text) != "on" {
        return;
    }

    // The pair lives inside an object literal — grab it.
    let Some(enclosing_object) = node.parent() else { return };
    if enclosing_object.kind() != "object" {
        return;
    }

    // Look at the key of the pair whose value is that enclosing object.
    let Some(outer_key) = enclosing_pair_key(enclosing_object, source) else { return };

    let message = match outer_key {
        "invoke" => "`on` inside an `invoke` configuration is ignored — move the transitions onto the surrounding state node.",
        "states" => "`on` directly under `states` is ignored — transitions belong on individual state nodes, not on the `states` map.",
        _ => return,
    };

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        message.to_string(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_on_inside_invoke() {
        let src = r#"
            const machine = createMachine({
                invoke: {
                    src: "fetchUser",
                    id: "user",
                    on: { DONE: "idle" },
                },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_on_directly_under_states() {
        let src = r#"
            const machine = createMachine({
                initial: "idle",
                states: {
                    idle: {},
                    on: { EVENT: "idle" },
                },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_on_inside_state_node() {
        let src = r#"
            const machine = createMachine({
                initial: "idle",
                states: {
                    idle: {
                        on: { EVENT: "active" },
                    },
                    active: {},
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_on_at_machine_root() {
        let src = r#"
            const machine = createMachine({
                on: {
                    GLOBAL: { target: ".idle" },
                },
                states: { idle: {} },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_invoke_without_on() {
        let src = r#"
            const machine = createMachine({
                invoke: {
                    src: "fetchUser",
                    onDone: { target: "success" },
                    onError: { target: "failure" },
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_on_inside_invoke_within_state_node() {
        let src = r#"
            const machine = createMachine({
                states: {
                    loading: {
                        invoke: {
                            src: "fetchUser",
                            on: { CANCEL: "idle" },
                        },
                    },
                },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
