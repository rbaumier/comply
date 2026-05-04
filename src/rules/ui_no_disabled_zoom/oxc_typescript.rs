//! ui-no-disabled-zoom OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName};
use oxc_span::GetSpan;
use std::sync::Arc;

fn content_disables_zoom(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    for part in lower.split(',') {
        let trimmed = part.trim();
        if trimmed.starts_with("user-scalable")
            && let Some((_, v)) = trimmed.split_once('=') {
                let val = v.trim();
                if val == "no" || val == "0" {
                    return true;
                }
            }
        if trimmed.starts_with("maximum-scale")
            && let Some((_, v)) = trimmed.split_once('=')
                && let Ok(scale) = v.trim().parse::<f64>()
                    && scale <= 1.0 {
                        return true;
                    }
    }
    false
}

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

        // Must be a <meta> tag.
        let JSXElementName::Identifier(tag_ident) = &opening.name else {
            return;
        };
        if tag_ident.name.as_str() != "meta" {
            return;
        }

        let mut is_viewport = false;
        let mut content_value = String::new();

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let attr_name = name_ident.name.as_str();

            match attr_name {
                "name" => {
                    if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value
                        && lit.value.as_str() == "viewport" {
                            is_viewport = true;
                        }
                }
                "content" => {
                    if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
                        content_value = lit.value.as_str().to_string();
                    }
                }
                _ => {}
            }
        }

        if is_viewport && content_disables_zoom(&content_value) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Viewport meta disables pinch-to-zoom — accessibility violation.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
