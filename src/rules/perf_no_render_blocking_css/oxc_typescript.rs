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

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["stylesheet"])
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
        // Must be a <link> element
        let tag_name = match &opening.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };
        if tag_name != "link" {
            return;
        }

        let mut rel: Option<&str> = None;
        let mut has_media = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            match name_ident.name.as_str() {
                "rel" => {
                    if let Some(JSXAttributeValue::StringLiteral(s)) = &attr.value {
                        rel = Some(s.value.as_str());
                    }
                }
                "media" => has_media = true,
                _ => {}
            }
        }

        if rel != Some("stylesheet") {
            return;
        }
        if has_media {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<link rel=\"stylesheet\">` without a `media` attribute blocks first paint — add `media=\"...\"` so the browser can defer non-critical CSS.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
