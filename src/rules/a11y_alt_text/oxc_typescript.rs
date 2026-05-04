//! a11y-alt-text oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
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

        let needs_alt = match tag {
            "img" | "area" => true,
            "input" => {
                // Only <input type="image"> needs alt.
                opening.attributes.iter().any(|item| {
                    let JSXAttributeItem::Attribute(attr) = item else {
                        return false;
                    };
                    let JSXAttributeName::Identifier(n) = &attr.name else {
                        return false;
                    };
                    if n.name.as_str() != "type" {
                        return false;
                    }
                    matches!(
                        &attr.value,
                        Some(JSXAttributeValue::StringLiteral(lit)) if lit.value.as_str() == "image"
                    )
                })
            }
            _ => false,
        };

        if !needs_alt {
            return;
        }

        // Check if alt= attribute exists.
        let has_alt = opening.attributes.iter().any(|item| {
            let JSXAttributeItem::Attribute(attr) = item else {
                return false;
            };
            let JSXAttributeName::Identifier(n) = &attr.name else {
                return false;
            };
            n.name.as_str() == "alt"
        });

        if !has_alt {
            let msg = match tag {
                "img" => "`<img>` is missing an `alt` attribute.",
                "area" => "`<area>` is missing an `alt` attribute.",
                _ => "`<input type=\"image\">` is missing an `alt` attribute.",
            };
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: msg.into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
