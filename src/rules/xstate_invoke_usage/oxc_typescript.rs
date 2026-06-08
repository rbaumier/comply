//! OxcCheck backend for xstate-invoke-usage.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

/// All properties XState accepts on an invoke object.
const VALID_INVOKE_PROPS: &[&str] = &[
    "src",
    "id",
    "input",
    "onDone",
    "onError",
    "onSnapshot",
    "systemId",
    "autoForward",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };

        let key_name = property_key_name(&prop.key);
        if key_name.as_deref() != Some("invoke") {
            return;
        }

        match &prop.value {
            Expression::ObjectExpression(obj) => {
                validate_invoke_object(obj, ctx, diagnostics);
            }
            Expression::ArrayExpression(arr) => {
                for elem in &arr.elements {
                    if let oxc_ast::ast::ArrayExpressionElement::ObjectExpression(obj) = elem {
                        validate_invoke_object(obj, ctx, diagnostics);
                    }
                }
            }
            other => {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, other.span().start as usize);
                let _kind = format!("{:?}", other);
                let kind_label = match other {
                    Expression::StringLiteral(_) => "string",
                    Expression::NumericLiteral(_) => "number",
                    _ => "expression",
                };
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`invoke` must be an object or array of objects — got `{kind_label}`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

fn property_key_name(key: &PropertyKey) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
        PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
        _ => None,
    }
}

fn validate_invoke_object(
    obj: &oxc_ast::ast::ObjectExpression,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut has_src = false;
    for prop_item in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = prop_item else {
            continue;
        };
        let Some(key) = property_key_name(&prop.key) else {
            continue;
        };
        if key == "src" {
            has_src = true;
        }
        if !VALID_INVOKE_PROPS.contains(&key.as_str()) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, prop.key.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{key}` is not a valid `invoke` property (allowed: {}).",
                    VALID_INVOKE_PROPS.join(", ")
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
    if !has_src {
        let (line, column) = byte_offset_to_line_col(ctx.source, obj.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`invoke` object is missing required `src` property.".into(),
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
    fn allows_invoke_with_src() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: {
                            src: 'fetchData',
                            onDone: 'success',
                            onError: 'failure',
                        },
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_invoke_array() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: [
                            { src: 'a' },
                            { src: 'b', onDone: 'ok' },
                        ],
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn flags_missing_src() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: { onDone: 'success' },
                    },
                },
            });
        "#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing required `src`"));
    }


    #[test]
    fn flags_unknown_invoke_prop() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: { src: 'fetchData', onDon: 'ok' },
                    },
                },
            });
        "#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("onDon"));
    }


    #[test]
    fn flags_invoke_as_string() {
        let src = r#"
            createMachine({
                states: {
                    loading: { invoke: 'fetchData' },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_all_known_invoke_props() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: {
                            src: 'fetch',
                            id: 'x',
                            input: {},
                            onDone: 'ok',
                            onError: 'err',
                            onSnapshot: 'snap',
                            systemId: 'sys',
                            autoForward: true,
                        },
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }
}
