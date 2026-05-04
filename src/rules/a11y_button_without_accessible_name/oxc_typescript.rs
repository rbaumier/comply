//! a11y-button-without-accessible-name OxcCheck backend.
//!
//! Flag `<button>` elements whose only children are SVG / icon components
//! (no readable text) and that lack `aria-label` / `aria-labelledby` / `title`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXChild, JSXElementName};
use std::sync::Arc;

pub struct Check;

fn is_icon_tag(tag: &str) -> bool {
    tag == "svg" || tag.ends_with("Icon") || tag.ends_with("Svg")
}

fn jsx_tag_str<'a>(name: &'a JSXElementName<'a>) -> Option<&'a str> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.as_str()),
        JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
        JSXElementName::MemberExpression(member) => Some(member.property.name.as_str()),
        JSXElementName::NamespacedName(ns) => Some(ns.name.name.as_str()),
        _ => None,
    }
}

fn child_provides_text(child: &JSXChild) -> bool {
    match child {
        JSXChild::Text(text) => !text.value.trim().is_empty(),
        JSXChild::Element(el) => {
            let tag = jsx_tag_str(&el.opening_element.name);
            tag.is_some_and(|t| !is_icon_tag(t))
        }
        // Expression children — assume they might render text; don't flag.
        JSXChild::ExpressionContainer(_) => true,
        JSXChild::Fragment(_) => true,
        JSXChild::Spread(_) => true,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["<button"])
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

        // Must be a <button> tag.
        let Some(tag) = jsx_tag_str(&opening.name) else {
            return;
        };
        if tag != "button" {
            return;
        }

        // Check for aria-label / aria-labelledby / title attributes.
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            if matches!(name.name.as_str(), "aria-label" | "aria-labelledby" | "title") {
                return;
            }
        }

        // Walk up to the parent JSXElement to inspect children.
        let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1) else {
            return;
        };
        let AstKind::JSXElement(element) = parent.kind() else {
            return;
        };

        // Check if any child provides text content.
        let any_text = element.children.iter().any(|c| child_provides_text(c));
        if any_text {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, element.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Icon-only `<button>` has no accessible name \u{2014} add `aria-label` or visible text.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
