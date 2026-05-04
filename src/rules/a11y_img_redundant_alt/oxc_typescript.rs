//! a11y-img-redundant-alt OxcCheck backend.
//!
//! Flags `<img>` elements whose `alt` text contains redundant words
//! like "image", "picture", or "photo".

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn has_redundant_word(alt: &str) -> bool {
    let lower = alt.to_ascii_lowercase();
    lower.contains("image") || lower.contains("picture") || lower.contains("photo")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["<img"])
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
        if tag != "img" {
            return;
        }

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            if name.name.as_str() != "alt" {
                continue;
            }
            let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
                continue;
            };
            if has_redundant_word(lit.value.as_str()) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span().start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`alt` text should not contain words like \"image\", \"picture\", or \"photo\" \u{2014} describe the content instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
