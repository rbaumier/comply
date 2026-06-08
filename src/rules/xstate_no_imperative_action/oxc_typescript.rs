//! xstate-no-imperative-action OXC backend — flag `send(...)` or `raise(...)`
//! called outside of an action context.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const ACTION_KEYS: &[&str] = &["actions", "entry", "exit"];

/// Walk ancestors looking for an ObjectProperty whose key is one of the
/// action context keys (`actions`, `entry`, `exit`).
fn is_inside_action_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::ObjectProperty(prop) = ancestor.kind() {
            let key_name = match &prop.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            if ACTION_KEYS.contains(&key_name) {
                return true;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let oxc_ast::ast::Expression::Identifier(ident) = &call.callee else {
            return;
        };
        let name = ident.name.as_str();
        if name != "send" && name != "raise" {
            return;
        }

        if is_inside_action_context(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{name}(...)` must be called inside an action (e.g. `actions: [{name}(...)]`), not imperatively."
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
