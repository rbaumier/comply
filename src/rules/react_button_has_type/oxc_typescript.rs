//! react-button-has-type oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
};
use std::sync::Arc;

const VALID_TYPES: &[&str] = &["button", "submit", "reset"];

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["<button"])
    }

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
        if tag_ident.name.as_str() != "button" {
            return;
        }

        let mut has_valid_type = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            if name_ident.name.as_str() != "type" {
                continue;
            }
            match &attr.value {
                Some(JSXAttributeValue::StringLiteral(lit)) => {
                    if VALID_TYPES.contains(&lit.value.as_str()) {
                        has_valid_type = true;
                    }
                }
                Some(_) => {
                    // Dynamic expression — assume valid.
                    has_valid_type = true;
                }
                None => {
                    // Bare `type` attribute — treat as present.
                    has_valid_type = true;
                }
            }
        }

        if !has_valid_type {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`<button>` missing an explicit `type` attribute \u{2014} \
                          defaults to `submit`, which may cause unexpected \
                          form submissions."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
