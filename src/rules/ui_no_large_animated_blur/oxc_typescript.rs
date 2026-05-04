//! ui-no-large-animated-blur oxc backend — flag inline `filter: blur(Npx)`
//! styles where the blur radius exceeds the configured max.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
    ObjectPropertyKind, PropertyKey,
};
use std::sync::Arc;

pub struct Check;

/// Parse the largest blur radius (in px) referenced inside `value`.
fn max_blur_px(value: &str) -> Option<f64> {
    let lower = value.to_ascii_lowercase();
    let mut max: Option<f64> = None;
    for (i, _) in lower.match_indices("blur(") {
        let rest = &lower[i + 5..];
        let Some(end) = rest.find(')') else { continue };
        let inner = rest[..end].trim();
        let num_part = inner.strip_suffix("px").unwrap_or(inner).trim();
        let Ok(n) = num_part.parse::<f64>() else {
            continue;
        };
        max = Some(max.map_or(n, |m| m.max(n)));
    }
    max
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["backdropFilter"])
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
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            if name.name.as_str() != "style" {
                continue;
            }
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) =
                &container.expression
            else {
                continue;
            };
            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(p) = prop else {
                    continue;
                };
                let key_name = match &p.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    PropertyKey::StringLiteral(s) => s.value.as_str(),
                    _ => continue,
                };
                if key_name != "filter"
                    && key_name != "backdropFilter"
                    && key_name != "WebkitBackdropFilter"
                {
                    continue;
                }
                let val_text = match &p.value {
                    Expression::StringLiteral(s) => s.value.as_str(),
                    Expression::TemplateLiteral(t) if t.quasis.len() == 1 => {
                        t.quasis[0].value.raw.as_str()
                    }
                    _ => continue,
                };
                let Some(radius) = max_blur_px(val_text) else {
                    continue;
                };
                let max_blur =
                    ctx.config
                        .float("ui-no-large-animated-blur", "max_blur_px", ctx.lang);
                if radius <= max_blur {
                    continue;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, p.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`blur({radius}px)` exceeds {max_blur}px — cost escalates with radius and \
                         layer size, can exhaust GPU memory on mobile."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
