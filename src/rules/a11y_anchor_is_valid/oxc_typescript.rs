//! a11y-anchor-is-valid OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName};
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
        if tag_ident.name.as_str() != "a" {
            return;
        }

        let mut has_href = false;
        let mut href_value: Option<String> = None;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            if name_ident.name.as_str() != "href" {
                continue;
            }
            has_href = true;
            if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
                href_value = Some(lit.value.to_string());
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);

        if !has_href {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Anchor is missing an `href` attribute.".into(),
                severity: Severity::Error,
                span: None,
            });
            return;
        }

        if let Some(val) = &href_value {
            if val == "#" {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Anchor has `href=\"#\"` — use a `<button>` or a real URL.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            } else if val.contains("javascript:") {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Anchor has `href=\"javascript:\"` — use a `<button>` or a real URL."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}
