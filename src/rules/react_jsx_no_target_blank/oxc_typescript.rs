//! react-jsx-no-target-blank OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["_blank"])
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

        // Scan attributes for target="_blank" and rel containing "noreferrer".
        let mut has_target_blank = false;
        let mut has_safe_rel = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let name = name_ident.name.as_str();
            let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
                continue;
            };
            let value = lit.value.as_str();

            match name {
                "target" => {
                    if value.contains("_blank") {
                        has_target_blank = true;
                    }
                }
                "rel" => {
                    if value.to_ascii_lowercase().contains("noreferrer") {
                        has_safe_rel = true;
                    }
                }
                _ => {}
            }
        }

        if !has_target_blank {
            return;
        }

        // Also check attributes on the parent JSXElement (for jsx_element style where
        // opening + children exist). The opening element already has the attrs.
        // For non-self-closing, check if rel is on the same opening element.
        if has_safe_rel {
            return;
        }

        let span_start = opening.span.start as usize;
        let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`target=\"_blank\"` without `rel=\"noreferrer\"` \
                      allows the opened page to access `window.opener`. \
                      Add `rel=\"noreferrer\"`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
