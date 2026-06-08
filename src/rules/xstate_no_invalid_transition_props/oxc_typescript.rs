//! xstate-no-invalid-transition-props OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

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

fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

/// Check if an ObjectExpression is a transition object: the value of
/// `on: { EVENT: <this object> }`.
fn is_transition_object(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();

    // Parent should be an ObjectProperty (the EVENT: { ... } pair)
    let event_prop_id = nodes.parent_id(node_id);
    if event_prop_id == node_id {
        return false;
    }
    let event_prop = nodes.get_node(event_prop_id);
    let AstKind::ObjectProperty(event_prop_ast) = event_prop.kind() else {
        return false;
    };
    // This object must be the value of the property
    // (not the key — the key is the event name)
    let _ = event_prop_ast;

    // Grandparent should be an ObjectExpression (the `on` object)
    let on_obj_id = nodes.parent_id(event_prop_id);
    if on_obj_id == event_prop_id {
        return false;
    }
    let on_obj = nodes.get_node(on_obj_id);
    if !matches!(on_obj.kind(), AstKind::ObjectExpression(_)) {
        return false;
    }

    // Great-grandparent should be an ObjectProperty with key "on"
    let on_prop_id = nodes.parent_id(on_obj_id);
    if on_prop_id == on_obj_id {
        return false;
    }
    let on_prop = nodes.get_node(on_prop_id);
    let AstKind::ObjectProperty(on_prop_ast) = on_prop.kind() else {
        return false;
    };
    let key_span = on_prop_ast.key.span();
    let key_text = &source[key_span.start as usize..key_span.end as usize];
    unquote(key_text) == "on"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };

        // The parent of this property must be a transition object
        let parent_id = semantic.nodes().parent_id(node.id());
        if parent_id == node.id() {
            return;
        }
        let parent = semantic.nodes().get_node(parent_id);
        if !matches!(parent.kind(), AstKind::ObjectExpression(_)) {
            return;
        }
        if !is_transition_object(parent_id, semantic, ctx.source) {
            return;
        }

        let key_span = prop.key.span();
        let key_text = &ctx.source[key_span.start as usize..key_span.end as usize];
        let key_unquoted = unquote(key_text);
        if key_unquoted.is_empty() {
            return;
        }
        if VALID_TRANSITION_PROPS.contains(&key_unquoted) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, key_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{key_unquoted}` is not a valid XState transition property (allowed: {}).",
                VALID_TRANSITION_PROPS.join(", ")
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
