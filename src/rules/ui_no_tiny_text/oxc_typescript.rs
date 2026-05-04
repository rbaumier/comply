//! ui-no-tiny-text oxc backend — flag inline `fontSize` numeric values below
//! the configured minimum (default 12px) inside JSX `style` attributes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, ObjectPropertyKind,
};
use std::sync::Arc;

pub struct Check;

/// Find the `style` attribute's object expression from a JSX opening element.
fn style_object<'a>(
    attrs: &'a oxc_allocator::Vec<'a, JSXAttributeItem<'a>>,
) -> Option<&'a oxc_ast::ast::ObjectExpression<'a>> {
    for item in attrs {
        let JSXAttributeItem::Attribute(attr) = item else { continue };
        let JSXAttributeName::Identifier(name) = &attr.name else { continue };
        if name.name.as_str() != "style" {
            continue;
        }
        // style={{ ... }} — the value is a JSXExpressionContainer wrapping an object
        if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
            if let oxc_ast::ast::JSXExpression::ObjectExpression(obj) = &container.expression {
                return Some(obj);
            }
        }
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fontSize"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let Some(obj) = style_object(&opening.attributes) else { return };

        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
            let Some(key_name) = p.key.static_name() else { continue };
            if key_name != "fontSize" {
                continue;
            }
            let Expression::NumericLiteral(num) = &p.value else { continue };

            let min_font = ctx.config.float("ui-no-tiny-text", "min_font_size_px", ctx.lang);
            if num.value >= min_font {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, p.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`fontSize: {}` \u{2014} values below {min_font}px are too small for \
                     comfortable reading.",
                    num.value
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
