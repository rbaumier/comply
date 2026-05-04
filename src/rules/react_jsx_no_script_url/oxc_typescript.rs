//! react-jsx-no-script-url oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
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

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            if !name_ident.name.as_str().eq_ignore_ascii_case("href") {
                continue;
            }

            let Some(value) = &attr.value else {
                continue;
            };

            let value_text = match value {
                JSXAttributeValue::StringLiteral(lit) => lit.value.as_str().to_string(),
                JSXAttributeValue::ExpressionContainer(container) => {
                    match &container.expression {
                        JSXExpression::StringLiteral(lit) => lit.value.as_str().to_string(),
                        JSXExpression::TemplateLiteral(tpl) => {
                            // Check quasis for javascript: prefix.
                            let start = tpl.span.start as usize;
                            let end = tpl.span.end as usize;
                            if end <= ctx.source.len() {
                                ctx.source[start..end].to_string()
                            } else {
                                continue;
                            }
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            };

            if value_text.to_ascii_lowercase().contains("javascript:") {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`javascript:` URLs are an XSS vector. Use an \
                              `onClick` handler instead."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}
