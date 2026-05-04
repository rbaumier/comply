//! a11y-no-noninteractive-tabindex oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName, JSXExpression,
    UnaryOperator,
};
use std::sync::Arc;

const NON_INTERACTIVE: &[&str] = &["div", "span", "p", "section"];

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

        // Only flag non-interactive HTML elements.
        let JSXElementName::Identifier(tag_ident) = &opening.name else {
            return;
        };
        let tag = tag_ident.name.as_str();
        if !NON_INTERACTIVE.contains(&tag) {
            return;
        }

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            if name_ident.name.as_str() != "tabIndex" {
                continue;
            }

            // Check if tabIndex value is -1 (allowed).
            let is_neg_one = match &attr.value {
                Some(JSXAttributeValue::StringLiteral(lit)) => lit.value.as_str() == "-1",
                Some(JSXAttributeValue::ExpressionContainer(container)) => {
                    if let JSXExpression::UnaryExpression(unary) = &container.expression {
                        if unary.operator == UnaryOperator::UnaryNegation {
                            if let oxc_ast::ast::Expression::NumericLiteral(num) = &unary.argument {
                                num.value == 1.0
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if !is_neg_one {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, opening.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Non-interactive element `<{tag}>` should not have `tabIndex`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
