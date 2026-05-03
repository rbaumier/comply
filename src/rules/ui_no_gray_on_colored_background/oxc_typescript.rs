//! ui-no-gray-on-colored-background — OXC backend.
//! Flags JSX `style={{ color: '#999', backgroundColor: '#0066cc' }}` where
//! the foreground is gray and the background is saturated.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
    ObjectPropertyKind, PropertyKey,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.strip_prefix('#')?;
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some((r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
    }
}

fn is_gray(r: u8, g: u8, b: u8) -> bool {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let diff = (max as i16 - min as i16).unsigned_abs() as u8;
    diff < 30 && max > 60 && max < 220
}

fn is_saturated(r: u8, g: u8, b: u8) -> bool {
    let max = r.max(g).max(b) as f64;
    let min = r.min(g).min(b) as f64;
    if max < 1.0 {
        return false;
    }
    let saturation = (max - min) / max;
    saturation > 0.4 && max > 80.0
}

fn extract_color_from_value(value: &str) -> Option<(u8, u8, u8)> {
    let v = value.trim().trim_matches(|c| c == '\'' || c == '"');
    if v.starts_with('#') {
        return parse_hex(v);
    }
    if let Some(inner) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse().ok()?;
            let g = parts[1].trim().parse().ok()?;
            let b = parts[2].trim().parse().ok()?;
            return Some((r, g, b));
        }
    }
    None
}

/// Extract the string value of a property by key name from an object expression.
fn get_property_string_value(
    props: &oxc_ast::ast::ObjectExpression<'_>,
    key_name: &str,
    source: &str,
) -> Option<String> {
    for prop in &props.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        let name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            _ => continue,
        };
        if name != key_name {
            continue;
        }
        let span = p.value.span();
        let text = &source[span.start as usize..span.end as usize];
        return Some(text.to_string());
    }
    None
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
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        // Find the `style` attribute.
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let JSXAttributeName::Identifier(name) = &attr.name else { continue };
            if name.name.as_str() != "style" {
                continue;
            }
            // style={expr} — the expr should be an object.
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) = &container.expression else { continue };

            let Some(color_str) = get_property_string_value(obj, "color", ctx.source) else {
                continue;
            };
            let Some(bg_str) =
                get_property_string_value(obj, "backgroundColor", ctx.source)
            else {
                continue;
            };

            let Some(fg) = extract_color_from_value(&color_str) else { continue };
            let Some(bg) = extract_color_from_value(&bg_str) else { continue };

            if is_gray(fg.0, fg.1, fg.2) && is_saturated(bg.0, bg.1, bg.2) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Gray text on colored background — low contrast, hard to read."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_gray_on_blue() {
        let src = r#"<p style={{ color: '#999', backgroundColor: '#0066cc' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_gray_on_red() {
        let src =
            r#"<p style={{ color: 'rgb(128, 128, 128)', backgroundColor: 'rgb(200, 50, 50)' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_white_on_blue() {
        let src = r#"<p style={{ color: '#ffffff', backgroundColor: '#0066cc' }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_gray_on_gray() {
        let src = r#"<p style={{ color: '#999', backgroundColor: '#eee' }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_background() {
        let src = r#"<p style={{ color: '#999' }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_style_object() {
        let src = r#"const obj = { color: '#999', backgroundColor: '#0066cc' };"#;
        // This is TSX-only rule checking JSXOpeningElement — plain objects are skipped.
        assert!(run(src).is_empty());
    }
}
