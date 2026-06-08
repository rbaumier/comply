use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["onDone"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::ObjectProperty(prop) = node.kind() else {
                continue;
            };

            let key_name = match &prop.key {
                PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };

            if key_name != "onDone" {
                continue;
            }

            // Walk up to find the parent ObjectExpression
            let parent_id = semantic.nodes().parent_id(node.id());
            if parent_id == node.id() {
                continue;
            }
            let parent_node = semantic.nodes().get_node(parent_id);
            let AstKind::ObjectExpression(parent_obj) = parent_node.kind() else {
                continue;
            };

            // Valid if the parent object has `states`, `invoke`, or `src` keys
            let has_valid_sibling = parent_obj.properties.iter().any(|p| {
                let ObjectPropertyKind::ObjectProperty(sibling) = p else {
                    return false;
                };
                let sib_key = match &sibling.key {
                    PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
                    PropertyKey::StringLiteral(s) => s.value.as_str(),
                    _ => return false,
                };
                matches!(sib_key, "states" | "invoke" | "src")
            });

            if has_valid_sibling {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, prop.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`onDone` only fires on compound states (with nested `states`) or invoking states (with `invoke`). This handler will never trigger.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
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

    use crate::diagnostic::Diagnostic;
    use super::Check;
}
