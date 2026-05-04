//! a11y-control-has-associated-label OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, JSXElementName,
    JSXExpression,
};
use std::sync::Arc;

const INTERACTIVE_ELEMENTS: &[&str] = &["button", "input", "select", "textarea"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let JSXElementName::Identifier(tag_ident) = &opening.name else {
            return;
        };
        let tag = tag_ident.name.as_str();

        if !INTERACTIVE_ELEMENTS.contains(&tag) {
            return;
        }

        // <input type="hidden"> is exempt
        if tag == "input" {
            for attr_item in &opening.attributes {
                let JSXAttributeItem::Attribute(attr) = attr_item else {
                    continue;
                };
                let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                    continue;
                };
                if name_ident.name.as_str() == "type"
                    && let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value
                        && lit.value.as_str() == "hidden" {
                            return;
                        }
            }
        }

        // Check for aria-label or aria-labelledby
        let has_label_attr = opening.attributes.iter().any(|attr_item| {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                return false;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                return false;
            };
            let name = name_ident.name.as_str();
            name == "aria-label" || name == "aria-labelledby"
        });
        if has_label_attr {
            return;
        }

        // For <button> elements, check parent JSXElement for text content
        if tag == "button"
            && let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1)
                && let AstKind::JSXElement(element) = parent.kind() {
                    let has_content = element.children.iter().any(|child| match child {
                        JSXChild::Text(text) => !text.value.trim().is_empty(),
                        JSXChild::Element(_) => true,
                        JSXChild::ExpressionContainer(ec) => {
                            !matches!(ec.expression, JSXExpression::EmptyExpression(_))
                        }
                        JSXChild::Fragment(_) => true,
                        JSXChild::Spread(_) => true,
                    });
                    if has_content {
                        return;
                    }
                }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Interactive element is missing an accessible label (`aria-label` or `aria-labelledby`).".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
