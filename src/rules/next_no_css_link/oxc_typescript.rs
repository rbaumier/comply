//! next-no-css-link OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName};
use std::sync::Arc;

const FONT_HOSTS: &[&str] = &["fonts.googleapis.com", "fonts.gstatic.com"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["<link"])
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
        if tag_ident.name.as_str() != "link" {
            return;
        }

        let mut rel_value: Option<&str> = None;
        let mut href_value: Option<&str> = None;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let name = name_ident.name.as_str();
            if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
                match name {
                    "rel" => rel_value = Some(lit.value.as_str()),
                    "href" => href_value = Some(lit.value.as_str()),
                    _ => {}
                }
            }
        }

        if rel_value != Some("stylesheet") {
            return;
        }

        // Defer to next-no-font-link for Google Fonts URLs.
        if let Some(href) = href_value
            && FONT_HOSTS.iter().any(|host| href.contains(host)) {
                return;
            }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<link rel=\"stylesheet\">` — import CSS directly so Next.js can bundle and optimize it.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
