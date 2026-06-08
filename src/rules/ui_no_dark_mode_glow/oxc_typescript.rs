//! OXC backend for ui-no-dark-mode-glow.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

fn perceived_lightness(r: u8, g: u8, b: u8) -> f64 {
    (0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64) / 255.0
}

fn parse_hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    match hex.len() {
        3 | 4 => Some((
            u8::from_str_radix(&hex[0..1], 16).ok()? * 17,
            u8::from_str_radix(&hex[1..2], 16).ok()? * 17,
            u8::from_str_radix(&hex[2..3], 16).ok()? * 17,
        )),
        6 | 8 => Some((
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        )),
        _ => None,
    }
}

fn parse_rgb_channels(value: &str) -> Option<(u8, u8, u8)> {
    let inner = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))?;
    let inner = inner.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() < 3 {
        return None;
    }
    let r: u8 = parts[0].trim().parse().ok()?;
    let g: u8 = parts[1].trim().parse().ok()?;
    let b: u8 = parts[2].trim().parse().ok()?;
    Some((r, g, b))
}

fn is_dark_color_value(value: &str) -> bool {
    let v = value.trim();
    if v.eq_ignore_ascii_case("black") {
        return true;
    }
    if let Some(hex) = v.strip_prefix('#')
        && let Some((r, g, b)) = parse_hex_rgb(hex) {
            return perceived_lightness(r, g, b) < 0.15;
        }
    if let Some((r, g, b)) = parse_rgb_channels(&v.to_ascii_lowercase()) {
        return perceived_lightness(r, g, b) < 0.15;
    }
    false
}

fn is_grayscale_rgb(r: u8, g: u8, b: u8) -> bool {
    let diff = |a: u8, b: u8| (a as i16 - b as i16).unsigned_abs();
    diff(r, g) < 10 && diff(r, b) < 10 && diff(g, b) < 10
}

fn shadow_has_chroma(shadow: &str) -> bool {
    let lower = shadow.to_ascii_lowercase();

    for (i, _) in lower.match_indices('#') {
        let rest = &lower[i + 1..];
        let hex_len = rest.chars().take_while(|c| c.is_ascii_hexdigit()).count();
        if hex_len >= 3
            && let Some((r, g, b)) = parse_hex_rgb(&rest[..hex_len])
                && !is_grayscale_rgb(r, g, b) {
                    return true;
                }
    }

    for prefix in &["rgba(", "rgb("] {
        for (i, _) in lower.match_indices(prefix) {
            let rest = &lower[i + prefix.len()..];
            if let Some(end) = rest.find(')') {
                let inner = &rest[..end];
                let parts: Vec<&str> = inner.split(',').collect();
                if parts.len() >= 3
                    && let (Ok(r), Ok(g), Ok(b)) = (
                        parts[0].trim().parse::<u8>(),
                        parts[1].trim().parse::<u8>(),
                        parts[2].trim().parse::<u8>(),
                    )
                        && !is_grayscale_rgb(r, g, b) {
                            return true;
                        }
            }
        }
    }

    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["boxShadow"])
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
            let oxc_ast::ast::JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };

            let attr_name = match &attr.name {
                oxc_ast::ast::JSXAttributeName::Identifier(id) => id.name.as_str(),
                _ => continue,
            };

            if attr_name != "style" {
                continue;
            }

            // Get the value — must be a JSX expression containing an object
            let Some(ref value) = attr.value else {
                continue;
            };
            let oxc_ast::ast::JSXAttributeValue::ExpressionContainer(container) = value else {
                continue;
            };
            let oxc_ast::ast::JSXExpression::ObjectExpression(ref obj) = container.expression else {
                continue;
            };

            if !has_dark_background(obj, ctx.source) || !has_colored_box_shadow(obj, ctx.source) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Colored glow shadow on a dark background — prefer subtle neutral shadows."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn property_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(lit) => Some(lit.value.as_str()),
        _ => None,
    }
}

fn string_value_of_expr<'a>(expr: &'a Expression<'a>, _source: &str) -> Option<String> {
    match expr {
        Expression::StringLiteral(lit) => Some(lit.value.to_string()),
        Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() && tpl.quasis.len() == 1 => {
            Some(tpl.quasis[0].value.raw.to_string())
        }
        _ => None,
    }
}

fn has_dark_background(obj: &oxc_ast::ast::ObjectExpression, source: &str) -> bool {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = prop else {
            continue;
        };
        let Some(key) = property_key_name(&prop.key) else {
            continue;
        };
        if key != "backgroundColor" && key != "background" {
            continue;
        }
        if let Some(val) = string_value_of_expr(&prop.value, source)
            && is_dark_color_value(&val) {
                return true;
            }
    }
    false
}

fn has_colored_box_shadow(obj: &oxc_ast::ast::ObjectExpression, source: &str) -> bool {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = prop else {
            continue;
        };
        let Some(key) = property_key_name(&prop.key) else {
            continue;
        };
        if key != "boxShadow" {
            continue;
        }
        if let Some(val) = string_value_of_expr(&prop.value, source)
            && shadow_has_chroma(&val) {
                return true;
            }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }


    #[test]
    fn flags_colored_glow_on_dark() {
        let src = r#"<div style={{
            backgroundColor: '#111',
            boxShadow: '0 0 20px rgba(0, 100, 255, 0.5)',
        }} />"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_neutral_shadow_on_dark() {
        let src = r#"<div style={{
            backgroundColor: '#111',
            boxShadow: '0 0 20px rgba(0, 0, 0, 0.3)',
        }} />"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_colored_shadow_on_light() {
        let src = r#"<div style={{
            backgroundColor: '#fff',
            boxShadow: '0 0 20px rgba(0, 100, 255, 0.5)',
        }} />"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_no_shadow() {
        let src = r#"<div style={{ backgroundColor: '#111' }} />"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_neutral_hex_shadow_on_dark() {
        let src = r#"<div style={{
            backgroundColor: '#111',
            boxShadow: '0 0 20px #000',
        }} />"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_bright_bg_with_rgb_zero_first_channel() {
        let src = r#"<div style={{
            backgroundColor: 'rgb(0, 200, 200)',
            boxShadow: '0 0 20px rgba(0, 100, 255, 0.5)',
        }} />"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn flags_hex_colored_shadow_on_dark() {
        let src = r#"<div style={{
            backgroundColor: '#0a0a0a',
            boxShadow: '0 0 20px #0064ff',
        }} />"#;
        assert_eq!(run(src).len(), 1);
    }
}
