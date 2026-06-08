//! xstate-no-misplaced-on-transition OXC backend — flag `on` properties that
//! live inside `invoke` or directly under `states`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectExpression(obj) = node.kind() else {
            return;
        };

        // Find `on` property inside this object.
        for prop in &obj.properties {
            let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue;
            };
            let key_name = match &p.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            if key_name != "on" {
                continue;
            }

            // Walk up to find what key this object is a value of.
            let outer_key = enclosing_property_key(node, semantic, ctx.source);
            let message = match outer_key.as_deref() {
                Some("invoke") => "`on` inside an `invoke` configuration is ignored — move the transitions onto the surrounding state node.",
                Some("states") => "`on` directly under `states` is ignored — transitions belong on individual state nodes, not on the `states` map.",
                _ => continue,
            };

            let (line, column) = byte_offset_to_line_col(ctx.source, p.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: message.into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Walk up the semantic tree from this object expression to find the property
/// key of the parent object property whose value is this object.
fn enclosing_property_key<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    _source: &str,
) -> Option<String> {
    // The parent of ObjectExpression should be an ObjectProperty whose value is this object.
    let parent_id = semantic.nodes().parent_id(node.id());
    if parent_id == node.id() {
        return None;
    }
    let parent = semantic.nodes().get_node(parent_id);
    let AstKind::ObjectProperty(prop) = parent.kind() else {
        return None;
    };
    let key = match &prop.key {
        oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.to_string(),
        oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.to_string(),
        _ => return None,
    };
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
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
