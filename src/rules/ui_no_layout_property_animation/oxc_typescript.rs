//! ui-no-layout-property-animation OXC backend — flag inline `transition` /
//! `transitionProperty` styles that animate layout-triggering properties.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXExpression, ObjectPropertyKind,
    PropertyKey,
};
use std::sync::Arc;

pub struct Check;

const TIMING_KEYS: &[&str] = &["transition", "transitionProperty"];

const LAYOUT_PROPERTIES: &[&str] = &[
    "width",
    "height",
    "min-width",
    "max-width",
    "min-height",
    "max-height",
    "top",
    "left",
    "right",
    "bottom",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border",
    "border-width",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
];

fn first_layout_property(value: &str) -> Option<&'static str> {
    let lower = value.to_ascii_lowercase();
    for token in lower.split(|c: char| !c.is_ascii_lowercase() && c != '-') {
        if token.is_empty() {
            continue;
        }
        for &prop in LAYOUT_PROPERTIES {
            if token == prop {
                return Some(prop);
            }
        }
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["transition", "transitionProperty"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let JSXAttributeName::Identifier(attr_name) = &attr.name else { continue };
            if attr_name.name.as_str() != "style" {
                continue;
            }

            let Some(ref value) = attr.value else { continue };
            let oxc_ast::ast::JSXAttributeValue::ExpressionContainer(container) = value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) = &container.expression else {
                continue;
            };

            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(pair) = prop else { continue };
                let key = match &pair.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    _ => continue,
                };
                if !TIMING_KEYS.contains(&key) {
                    continue;
                }

                let value_str = match &pair.value {
                    Expression::StringLiteral(s) => s.value.as_str(),
                    _ => continue,
                };

                let Some(layout_prop) = first_layout_property(value_str) else { continue };

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, pair.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Animating layout property `{layout_prop}` triggers layout recalculation every frame — \
                         prefer `transform`, `opacity`, `color`, `background`, or `filter`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
