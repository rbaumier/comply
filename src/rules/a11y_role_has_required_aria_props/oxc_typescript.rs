//! a11y-role-has-required-aria-props OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue};
use std::sync::Arc;

/// Returns the required ARIA props for a given role.
fn required_props(role: &str) -> &'static [&'static str] {
    match role {
        "checkbox" | "radio" => &["aria-checked"],
        "slider" => &["aria-valuenow", "aria-valuemin", "aria-valuemax"],
        "combobox" => &["aria-expanded"],
        "scrollbar" => &["aria-controls", "aria-valuenow"],
        _ => &[],
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["role"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let mut role_value: Option<&str> = None;
        let mut present_attrs: Vec<&str> = Vec::new();

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let name = name_ident.name.as_str();
            present_attrs.push(name);
            if name == "role"
                && let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
                    role_value = Some(lit.value.as_str());
                }
        }

        let Some(role) = role_value else { return };
        let props = required_props(role);
        if props.is_empty() {
            return;
        }

        let missing: Vec<&str> = props
            .iter()
            .filter(|prop| !present_attrs.iter().any(|a| a == *prop))
            .copied()
            .collect();

        if !missing.is_empty() {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`role=\"{}\"` is missing required ARIA props: {}.",
                    role,
                    missing.join(", ")
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
