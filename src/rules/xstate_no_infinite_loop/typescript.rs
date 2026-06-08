//! xstate-no-infinite-loop backend — flag `always` (eventless) transitions
//! that stay in, or re-target, the same state without a guard.
//!
//! XState evaluates `always` transitions immediately upon entering a state.
//! A transition with no `target` (self-loop) or one that targets the enclosing
//! state, and no `guard`/`cond`, will re-fire forever and hang the interpreter.

use crate::diagnostic::{Diagnostic, Severity};

/// Strip matching surrounding quotes from an identifier/string literal.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '\'' || c == '"' || c == '`')
}

/// Return the key text of a `pair` node (unquoted), or `""` if absent.
fn pair_key<'a>(pair: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    pair.child_by_field_name("key")
        .and_then(|k| k.utf8_text(source).ok())
        .map(unquote)
        .unwrap_or("")
}

/// Walk up to find the enclosing state name. A state name is the key of the
/// nearest ancestor `pair` whose grandparent pair has key `states`.
fn enclosing_state_name<'a>(
    always_pair: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<&'a str> {
    let mut current = always_pair.parent();
    while let Some(node) = current {
        if node.kind() == "pair" {
            // Grandparent pair: pair -> object -> pair
            let gp = node.parent().and_then(|o| o.parent());
            if let Some(gp) = gp
                && gp.kind() == "pair"
                && pair_key(gp, source) == "states"
            {
                return Some(pair_key(node, source));
            }
        }
        current = node.parent();
    }
    None
}

/// Iterate over object-literal transition candidates inside the `always` value.
/// Calls `visit` with each candidate object node.
fn for_each_transition_object<'a>(
    value: tree_sitter::Node<'a>,
    mut visit: impl FnMut(tree_sitter::Node<'a>),
) {
    match value.kind() {
        "object" => visit(value),
        "array" => {
            let mut cursor = value.walk();
            for child in value.named_children(&mut cursor) {
                if child.kind() == "object" {
                    visit(child);
                }
            }
        }
        _ => {}
    }
}

/// Look up a property pair by name inside an `object` node.
fn find_property<'a>(
    object: tree_sitter::Node<'a>,
    source: &'a [u8],
    name: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = object.walk();
    object
        .named_children(&mut cursor)
        .find(|child| child.kind() == "pair" && pair_key(*child, source) == name)
}

/// True if the transition object would cause an infinite loop: no guard/cond,
/// and either no target (self-loop) or target equals the enclosing state.
fn is_infinite(obj: tree_sitter::Node, source: &[u8], enclosing_state: Option<&str>) -> bool {
    if find_property(obj, source, "guard").is_some() || find_property(obj, source, "cond").is_some()
    {
        return false;
    }

    let target = find_property(obj, source, "target");
    match target {
        None => true,
        Some(pair) => {
            let Some(value) = pair.child_by_field_name("value") else {
                return true;
            };
            let text = unquote(value.utf8_text(source).unwrap_or(""));
            match enclosing_state {
                Some(state) => text == state,
                None => false,
            }
        }
    }
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if pair_key(node, source) != "always" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };

    let enclosing = enclosing_state_name(node, source);

    for_each_transition_object(value, |obj| {
        if !is_infinite(obj, source, enclosing) {
            return;
        }
        let pos = obj.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "`always` transition has no guard and stays in the same state — this will loop forever. Add a `guard`/`cond` or target a different state.".into(),
            severity: Severity::Error,
            span: None,
        });
    });
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
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_unguarded_self_loop_no_target() {
        let src = r#"
            const machine = createMachine({
                states: {
                    idle: {
                        always: [
                            { actions: 'doSomething' },
                        ],
                    },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_unguarded_explicit_self_target() {
        let src = r#"
            const machine = createMachine({
                states: {
                    idle: {
                        always: [
                            { target: 'idle', actions: 'doSomething' },
                        ],
                    },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_guarded_always_transition() {
        let src = r#"
            const machine = createMachine({
                states: {
                    idle: {
                        always: [
                            { guard: 'isReady', actions: 'doSomething' },
                        ],
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_cond_legacy_guard() {
        let src = r#"
            const machine = createMachine({
                states: {
                    idle: {
                        always: [
                            { cond: 'isReady', target: 'idle' },
                        ],
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_always_with_different_target() {
        let src = r#"
            const machine = createMachine({
                states: {
                    idle: {
                        always: [
                            { target: 'next' },
                        ],
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_single_object_always_without_guard_or_target() {
        let src = r#"
            const machine = createMachine({
                states: {
                    idle: {
                        always: { actions: 'doSomething' },
                    },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }
}
