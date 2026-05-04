//! react-no-invalid-html-attribute oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName};
use std::sync::Arc;

/// Valid `rel` values for `<a>` elements.
const VALID_A_RELS: &[&str] = &[
    "alternate",
    "author",
    "bookmark",
    "external",
    "help",
    "license",
    "next",
    "nofollow",
    "noopener",
    "noreferrer",
    "opener",
    "prev",
    "search",
    "tag",
    "ugc",
    "sponsored",
];

/// Valid `rel` values for `<link>` elements.
const VALID_LINK_RELS: &[&str] = &[
    "alternate",
    "author",
    "canonical",
    "dns-prefetch",
    "help",
    "icon",
    "license",
    "manifest",
    "modulepreload",
    "next",
    "pingback",
    "preconnect",
    "prefetch",
    "preload",
    "prerender",
    "prev",
    "search",
    "shortlink",
    "stylesheet",
    "apple-touch-icon",
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

        let valid_rels = match tag {
            "a" => VALID_A_RELS,
            "link" => VALID_LINK_RELS,
            _ => return,
        };

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(attr_ident) = &attr.name else {
                continue;
            };
            if attr_ident.name.as_str() != "rel" {
                continue;
            }
            let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
                continue;
            };
            let val = lit.value.as_str();

            for token in val.split_whitespace() {
                if !valid_rels.contains(&token) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!("Invalid `rel` value `{token}` on `<{tag}>`."),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
    }
}
