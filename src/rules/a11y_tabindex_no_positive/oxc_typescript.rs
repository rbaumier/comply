//! a11y-tabindex-no-positive oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
use std::sync::Arc;

pub struct Check;

fn is_positive_tabindex(value: &JSXAttributeValue) -> bool {
    match value {
        // String literal: "N"
        JSXAttributeValue::StringLiteral(lit) => {
            if let Ok(n) = lit.value.as_str().trim().parse::<i32>() {
                return n > 0;
            }
            false
        }
        // JSX expression: {N}
        JSXAttributeValue::ExpressionContainer(container) => {
            match &container.expression {
                JSXExpression::NumericLiteral(num) => num.value > 0.0,
                JSXExpression::UnaryExpression(un) => {
                    if un.operator == oxc_ast::ast::UnaryOperator::UnaryNegation {
                        if let oxc_ast::ast::Expression::NumericLiteral(num) = &un.argument {
                            return (-num.value) > 0.0;
                        }
                    }
                    false
                }
                _ => false,
            }
        }
        _ => false,
    }
}

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
            if name_ident.name.as_str() != "tabIndex" {
                continue;
            }
            let Some(value) = &attr.value else {
                continue;
            };
            if is_positive_tabindex(value) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`tabIndex` must not be positive — use `0` or `-1` only.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}
