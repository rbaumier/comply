//! react-no-danger-with-children oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXChild, JSXExpression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["dangerouslySetInnerHTML"])
    }

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

        let mut has_danger = false;
        let mut has_children_prop = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(attr_ident) = &attr.name else {
                continue;
            };
            match attr_ident.name.as_str() {
                "dangerouslySetInnerHTML" => has_danger = true,
                "children" => has_children_prop = true,
                _ => {}
            }
        }

        if !has_danger {
            return;
        }

        // For non-self-closing elements, check for text children.
        let has_text_children = if let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1) {
            if let AstKind::JSXElement(element) = parent.kind() {
                if element.closing_element.is_some() {
                    element.children.iter().any(|child| match child {
                        JSXChild::Text(text) => !text.value.trim().is_empty(),
                        JSXChild::Element(_) => true,
                        JSXChild::ExpressionContainer(ec) => {
                            !matches!(ec.expression, JSXExpression::EmptyExpression(_))
                        }
                        JSXChild::Fragment(_) => true,
                        JSXChild::Spread(_) => true,
                    })
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        if has_children_prop || has_text_children {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Using both `dangerouslySetInnerHTML` and \
                          `children` on the same element is invalid — \
                          React will throw at runtime."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
