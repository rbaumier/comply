//! a11y-aria-unsupported-elements oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName};
use std::sync::Arc;

const UNSUPPORTED_ELEMENTS: &[&str] = &[
    "meta", "html", "script", "style", "head", "title", "link", "base",
];

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

        let tag = match &opening.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };

        if !UNSUPPORTED_ELEMENTS.contains(&tag) {
            return;
        }

        let has_aria_or_role = opening.attributes.iter().any(|attr_item| {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                return false;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                return false;
            };
            let name = name_ident.name.as_str();
            name.starts_with("aria-") || name == "role"
        });

        if has_aria_or_role {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "ARIA attributes and `role` are not supported on this element.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
