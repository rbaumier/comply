//! a11y-media-has-caption OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, JSXElementName,
};
use std::sync::Arc;

/// Walk children of a JSXElement looking for `<track kind="captions" />`.
fn has_caption_track(children: &oxc_allocator::Vec<'_, JSXChild<'_>>) -> bool {
    for child in children.iter() {
        if let JSXChild::Element(el) = child {
            // Check opening element
            if is_track_with_captions(&el.opening_element) {
                return true;
            }
            // Recurse into children
            if has_caption_track(&el.children) {
                return true;
            }
        }
    }
    false
}

fn is_track_with_captions(opening: &oxc_ast::ast::JSXOpeningElement) -> bool {
    let JSXElementName::Identifier(ident) = &opening.name else {
        return false;
    };
    if ident.name.as_str() != "track" {
        return false;
    }
    for attr_item in &opening.attributes {
        let JSXAttributeItem::Attribute(attr) = attr_item else {
            continue;
        };
        let JSXAttributeName::Identifier(name_ident) = &attr.name else {
            continue;
        };
        if name_ident.name.as_str() == "kind"
            && let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value
                && lit.value.as_str() == "captions" {
                    return true;
                }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXElement(element) = node.kind() else {
            return;
        };

        let JSXElementName::Identifier(tag_ident) = &element.opening_element.name else {
            return;
        };
        let tag = tag_ident.name.as_str();
        if tag != "video" && tag != "audio" {
            return;
        }

        // Self-closing: no children possible
        if element.closing_element.is_none() {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, element.opening_element.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`<{tag}>` elements must have a `<track kind=\"captions\">` child for accessibility."),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        if !has_caption_track(&element.children) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, element.opening_element.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`<{tag}>` elements must have a `<track kind=\"captions\">` child for accessibility."),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
