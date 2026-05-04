//! react-no-constructed-context-values OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName, JSXExpression,
};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Provider"])
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

        // Tag must contain "Provider".
        let tag_str = match &opening.name {
            JSXElementName::Identifier(id) => id.name.as_str().to_string(),
            JSXElementName::MemberExpression(member) => {
                format!("{}.{}", member.object, member.property.name)
            }
            _ => return,
        };
        if !tag_str.contains("Provider") {
            return;
        }

        // Find the `value` attribute.
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            if name_ident.name.as_str() != "value" {
                continue;
            }

            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };

            let is_inline = match &container.expression {
                JSXExpression::ObjectExpression(_) | JSXExpression::ArrayExpression(_) => true,
                _ => false,
            };

            if is_inline {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Context Provider `value` is an inline object/array — \
                              a new reference is created every render, causing all \
                              consumers to re-render. Memoize with `useMemo`."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
