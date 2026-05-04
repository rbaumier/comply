//! react-jsx-no-bind OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
use oxc_span::GetSpan;
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

            // Get the attribute name
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let attr_name = name_ident.name.as_str();

            // Value must be an expression container
            let Some(JSXAttributeValue::ExpressionContainer(ec)) = &attr.value else {
                continue;
            };

            let expr = match &ec.expression {
                JSXExpression::EmptyExpression(_) => continue,
                other => other,
            };

            let (kind_label, span) = match expr {
                JSXExpression::ArrowFunctionExpression(arrow) => {
                    ("arrow function", arrow.span)
                }
                JSXExpression::FunctionExpression(func) => {
                    ("function expression", func.span)
                }
                JSXExpression::CallExpression(call) => {
                    // Detect `foo.bind(...)`
                    let Expression::StaticMemberExpression(member) = &call.callee else {
                        continue;
                    };
                    if member.property.name.as_str() != "bind" {
                        continue;
                    }
                    ("`.bind()` call", call.span())
                }
                _ => continue,
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "{kind_label} as value of JSX prop `{attr_name}` creates a new reference every render \u{2014} hoist to `useCallback` or a stable handler."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
