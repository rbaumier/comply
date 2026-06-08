//! xstate-no-ondone-outside-compound-state backend — flag `onDone` handlers on
//! state nodes that are neither compound (have nested `states`) nor invoking
//! (have an `invoke` property).
//!
//! In XState, `onDone` fires when a compound state reaches a final substate or
//! when an invoked actor terminates. Placing `onDone` on an atomic state node
//! never triggers and silently hides logic bugs — the transition is dead code.

use crate::diagnostic::{Diagnostic, Severity};

/// Reads the key text of a `pair` node, stripping surrounding quotes.
fn pair_key_text<'a>(pair: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let key = pair.child_by_field_name("key")?;
    let text = key.utf8_text(source).ok()?;
    Some(text.trim_matches(|c: char| c == '\'' || c == '"' || c == '`'))
}

/// True if `object` contains a `pair` child whose key matches one of `names`.
fn object_has_key(object: tree_sitter::Node, source: &[u8], names: &[&str]) -> bool {
    let mut cursor = object.walk();
    for child in object.children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        if let Some(key) = pair_key_text(child, source)
            && names.contains(&key)
        {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["pair"] prefilter = ["onDone"] => |node, source, ctx, diagnostics|
    let Some(key_text) = pair_key_text(node, source) else { return };
    if key_text != "onDone" {
        return;
    }

    // Parent must be an object literal — walk up to find it.
    let Some(parent) = node.parent() else { return };
    if parent.kind() != "object" {
        return;
    }

    // Valid on:
    // - compound states: object has a `states` sibling
    // - invoking states: object has an `invoke` sibling
    // - invoke config objects themselves: sibling `src` marks the object as
    //   an invoke descriptor whose `onDone` is the promise-completion handler.
    if object_has_key(parent, source, &["states", "invoke", "src"]) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`onDone` only fires on compound states (with nested `states`) or invoking states (with `invoke`). This handler will never trigger.".to_string(),
        Severity::Warning,
    ));
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_ondone_on_atomic_state() {
        let src = r#"
            createMachine({
                states: {
                    idle: {
                        onDone: { target: 'done' },
                    },
                },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ondone_on_state_with_only_on_transitions() {
        let src = r#"
            createMachine({
                states: {
                    idle: {
                        on: { EVENT: 'next' },
                        onDone: 'finished',
                    },
                },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ondone_on_compound_state() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        states: {
                            fetching: {},
                            parsing: {},
                        },
                        onDone: 'ready',
                    },
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ondone_on_invoking_state() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: { src: 'fetchUser' },
                        onDone: 'ready',
                    },
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ondone_as_invoke_callback() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: {
                            src: 'fetchUser',
                            onDone: { target: 'ready' },
                        },
                    },
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ondone_on_state_with_both_states_and_invoke() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: { src: 'fetchUser' },
                        states: {
                            polling: {},
                        },
                        onDone: 'ready',
                    },
                },
            });
        "#;
        assert!(run(src).is_empty());
    }
}
