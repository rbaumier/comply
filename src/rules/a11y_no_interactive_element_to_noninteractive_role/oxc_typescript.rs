//! a11y-no-interactive-element-to-noninteractive-role OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
};
use std::sync::Arc;

pub struct Check;

const INTERACTIVE_ELEMENTS: &[&str] = &["button", "a", "input", "select", "textarea"];

const NON_INTERACTIVE_ROLES: &[&str] = &[
    "article",
    "banner",
    "complementary",
    "contentinfo",
    "document",
    "img",
    "list",
    "listitem",
    "note",
    "presentation",
    "none",
    "heading",
];

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

        let tag = match &opening.name {
            JSXElementName::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !INTERACTIVE_ELEMENTS.contains(&tag) {
            return;
        }

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            if name.name.as_str() != "role" {
                continue;
            }
            let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
                continue;
            };
            let role = lit.value.as_str();
            if NON_INTERACTIVE_ROLES.contains(&role) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, opening.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Interactive element should not have non-interactive `role=\"{role}\"`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
