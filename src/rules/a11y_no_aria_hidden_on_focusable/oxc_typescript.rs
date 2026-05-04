//! a11y-no-aria-hidden-on-focusable OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName, JSXExpression,
};
use std::sync::Arc;

const FOCUSABLE_TAGS: &[&str] = &["button", "a", "input", "select", "textarea"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["aria-hidden"])
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

        let JSXElementName::Identifier(tag_ident) = &opening.name else {
            return;
        };
        let tag = tag_ident.name.as_str();

        let mut has_aria_hidden = false;
        let mut has_tabindex = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let name = name_ident.name.as_str();

            if name == "aria-hidden" {
                // Check value is truthy
                match &attr.value {
                    None => {
                        // Bare `aria-hidden` without value — treated as true
                        has_aria_hidden = true;
                    }
                    Some(JSXAttributeValue::StringLiteral(lit)) => {
                        if lit.value.as_str() == "true" {
                            has_aria_hidden = true;
                        }
                    }
                    Some(JSXAttributeValue::ExpressionContainer(ec)) => {
                        if let JSXExpression::BooleanLiteral(b) = &ec.expression {
                            if b.value {
                                has_aria_hidden = true;
                            }
                        } else {
                            // Check source text for {true}
                            let start = ec.span.start as usize;
                            let end = ec.span.end as usize;
                            if end <= ctx.source.len() {
                                let text = &ctx.source[start..end];
                                let inner = text
                                    .trim_start_matches('{')
                                    .trim_end_matches('}')
                                    .trim();
                                if inner == "true" {
                                    has_aria_hidden = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            if name == "tabIndex" || name == "tabindex" {
                has_tabindex = true;
            }
        }

        if !has_aria_hidden {
            return;
        }

        let is_focusable = FOCUSABLE_TAGS.contains(&tag) || has_tabindex;
        if is_focusable {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`aria-hidden=\"true\"` must not be set on focusable elements.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
