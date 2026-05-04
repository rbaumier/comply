//! a11y-prefer-tag-over-role oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName};
use std::sync::Arc;

/// (role value, suggested element)
const ROLE_TO_TAG: &[(&str, &str)] = &[
    ("button", "<button>"),
    ("link", "<a>"),
    ("img", "<img>"),
    ("heading", "<h1>-<h6>"),
    ("navigation", "<nav>"),
    ("banner", "<header>"),
    ("contentinfo", "<footer>"),
    ("main", "<main>"),
];

const GENERIC_ELEMENTS: &[&str] = &["div", "span"];

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
        if !GENERIC_ELEMENTS.contains(&tag) {
            return;
        }

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            if name_ident.name.as_str() != "role" {
                continue;
            }
            let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
                continue;
            };
            let role = lit.value.as_str();
            for &(mapped_role, suggested) in ROLE_TO_TAG {
                if role == mapped_role {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, opening.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Prefer `{suggested}` over `<{tag} role=\"{role}\">` for semantic HTML."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
    }
}
