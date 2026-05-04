//! a11y-anchor-has-content oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXChild, JSXElementName, JSXExpression,
};
use std::sync::Arc;

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

        // Only check `<a>` tags.
        let JSXElementName::Identifier(ident) = &opening.name else {
            return;
        };
        if ident.name.as_str() != "a" {
            return;
        }

        // Check for aria-label / aria-labelledby attribute.
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(attr_ident) = &attr.name else {
                continue;
            };
            let name = attr_ident.name.as_str();
            if name == "aria-label" || name == "aria-labelledby" {
                return;
            }
        }

        // Walk up to parent JSXElement.
        let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1) else {
            return;
        };
        let AstKind::JSXElement(element) = parent.kind() else {
            return;
        };

        // Self-closing: `<a ... />`
        if element.closing_element.is_none() {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Anchor is self-closing and has no content for screen readers.".into(),
                severity: Severity::Error,
                span: None,
            });
            return;
        }

        // Non-self-closing: check children for content.
        let has_content = element.children.iter().any(|child| match child {
            JSXChild::Text(text) => !text.value.trim().is_empty(),
            JSXChild::Element(_) => true,
            JSXChild::ExpressionContainer(ec) => {
                !matches!(ec.expression, JSXExpression::EmptyExpression(_))
            }
            JSXChild::Fragment(_) => true,
            JSXChild::Spread(_) => true,
        });

        if !has_content {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Anchor has no content — screen readers cannot announce it.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
