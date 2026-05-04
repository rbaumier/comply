use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeValue, JSXExpression, ObjectPropertyKind,
    PropertyKey,
};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["willChange"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(jsx) = node.kind() else { return };

        for attr_item in &jsx.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };

            // Must be `style` attribute
            let attr_name = &attr.name;
            let oxc_ast::ast::JSXAttributeName::Identifier(name_id) = attr_name else {
                continue;
            };
            if name_id.name != "style" {
                continue;
            }

            // Value must be a JSX expression container with an object
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) =
                &container.expression
            else {
                continue;
            };

            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
                let key_name = match &p.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    PropertyKey::StringLiteral(s) => s.value.as_str(),
                    _ => continue,
                };
                if key_name != "willChange" {
                    continue;
                }

                // Allow "auto"
                if let Expression::StringLiteral(s) = &p.value
                    && s.value == "auto" {
                        continue;
                    }

                let span = p.span;
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Permanent `willChange` wastes GPU memory — apply only during active animation and remove after.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
