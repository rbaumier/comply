//! Flags inline JSX style objects with gray `color` + saturated `backgroundColor`.

use crate::diagnostic::{Diagnostic, Severity};

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

fn get_pair_string_value<'a>(
    obj: tree_sitter::Node<'a>,
    source: &'a [u8],
    key: &str,
) -> Option<String> {
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let k = child
            .child_by_field_name("key")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        if k != key {
            continue;
        }
        let val = child.child_by_field_name("value")?;
        if val.kind() == "string" || val.kind() == "template_string" {
            return val.utf8_text(source).ok().map(|s| s.to_string());
        }
    }
    None
}

fn is_in_style_jsx_attribute(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(obj) = node.parent() else {
        return false;
    };
    if obj.kind() != "object" {
        return false;
    }
    let Some(jsx_expr) = obj.parent() else {
        return false;
    };
    if jsx_expr.kind() != "jsx_expression" {
        return false;
    }
    let Some(jsx_attr) = jsx_expr.parent() else {
        return false;
    };
    if jsx_attr.kind() != "jsx_attribute" {
        return false;
    }
    crate::rules::jsx::jsx_attribute_name(jsx_attr, source) == Some("style")
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let k = node.child_by_field_name("key")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("");
    if k != "color" {
        return;
    }
    if !is_in_style_jsx_attribute(node, source) {
        return;
    }
    let Some(obj) = node.parent() else { return };
    let Some(color_str) = get_pair_string_value(obj, source, "color") else { return };
    let Some(bg_str) = get_pair_string_value(obj, source, "backgroundColor") else { return };

    let Some(fg) = extract_color_from_value(&color_str) else { return };
    let Some(bg) = extract_color_from_value(&bg_str) else { return };

    if is_gray(fg.0, fg.1, fg.2) && is_saturated(bg.0, bg.1, bg.2) {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: node.start_position().row + 1,
            column: node.start_position().column + 1,
            rule_id: super::META.id.into(),
            message: "Gray text on colored background — low contrast, hard to read.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_gray_on_blue() {
        let src = r#"<p style={{ color: '#999', backgroundColor: '#0066cc' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_gray_on_red() {
        let src = r#"<p style={{ color: 'rgb(128, 128, 128)', backgroundColor: 'rgb(200, 50, 50)' }} />"#;
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
        assert!(run(src).is_empty());
    }
}
