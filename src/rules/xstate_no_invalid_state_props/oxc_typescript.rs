use std::sync::Arc;

use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

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

fn property_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(lit) => Some(lit.value.as_str()),
        _ => None,
    }
}

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
        // Anchor on `states: { ... }` properties.
        let Some(key_name) = property_key_name(&prop.key) else {
            return;
        };
        if key_name != "states" {
            return;
        }
        let Expression::ObjectExpression(states_obj) = &prop.value else {
            return;
        };

        // Each property of `states: { ... }` is a state node.
        for state_prop_kind in &states_obj.properties {
            let ObjectPropertyKind::ObjectProperty(state_prop) = state_prop_kind else {
                continue;
            };
            let Expression::ObjectExpression(state_obj) = &state_prop.value else {
                continue;
            };

            // Each property inside the state node object.
            for inner_kind in &state_obj.properties {
                let ObjectPropertyKind::ObjectProperty(inner_prop) = inner_kind else {
                    continue;
                };
                let Some(prop_text) = property_key_name(&inner_prop.key) else {
                    continue;
                };
                if prop_text.is_empty() {
                    continue;
                }
                if VALID_STATE_PROPS.contains(&prop_text) {
                    continue;
                }

                let span_start = inner_prop.key.span().start;
                let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
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
}
