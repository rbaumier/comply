use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["preload"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        // Only match `<link ...>`
        let oxc_ast::ast::JSXElementName::Identifier(tag_ident) = &opening.name else { return };
        if tag_ident.name.as_str() != "link" {
            return;
        }

        let mut rel: Option<&str> = None;
        let mut as_attr: Option<&str> = None;
        let mut has_crossorigin = false;
        let mut type_attr: Option<&str> = None;

        for attr_item in &opening.attributes {
            let oxc_ast::ast::JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let oxc_ast::ast::JSXAttributeName::Identifier(name_ident) = &attr.name else { continue };
            let name = name_ident.name.as_str();
            match name {
                "rel" => {
                    if let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(s)) = &attr.value {
                        rel = Some(s.value.as_str());
                    }
                }
                "as" => {
                    if let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(s)) = &attr.value {
                        as_attr = Some(s.value.as_str());
                    }
                }
                "crossOrigin" | "crossorigin" => {
                    has_crossorigin = true;
                }
                "type" => {
                    if let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(s)) = &attr.value {
                        type_attr = Some(s.value.as_str());
                    }
                }
                _ => {}
            }
        }

        // Only applies to `<link rel="preload" as="font">`
        if rel != Some("preload") || as_attr != Some("font") {
            return;
        }

        let missing_cors = !has_crossorigin;
        let missing_type = type_attr != Some("font/woff2");

        if missing_cors || missing_type {
            let msg = match (missing_cors, missing_type) {
                (true, true) => "Font preload `<link>` is missing both `crossorigin` and `type=\"font/woff2\"`.",
                (true, false) => "Font preload `<link>` is missing `crossorigin` \u{2014} fonts are fetched in CORS mode.",
                (false, true) => "Font preload `<link>` should declare `type=\"font/woff2\"` so the preload matches the CSSOM request.",
                (false, false) => unreachable!(),
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: msg.into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
