//! Flags inline style objects with a dark backgroundColor AND a colored
//! boxShadow (non-grayscale shadow on a dark background).

use crate::diagnostic::{Diagnostic, Severity};

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
    if let Some(hex) = v.strip_prefix('#') {
        if let Some((r, g, b)) = parse_hex_rgb(hex) {
            return perceived_lightness(r, g, b) < 0.15;
        }
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
        if hex_len >= 3 {
            if let Some((r, g, b)) = parse_hex_rgb(&rest[..hex_len]) {
                if !is_grayscale_rgb(r, g, b) {
                    return true;
                }
            }
        }
    }

    for prefix in &["rgba(", "rgb("] {
        for (i, _) in lower.match_indices(prefix) {
            let rest = &lower[i + prefix.len()..];
            if let Some(end) = rest.find(')') {
                let inner = &rest[..end];
                let parts: Vec<&str> = inner.split(',').collect();
                if parts.len() >= 3 {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        parts[0].trim().parse::<u8>(),
                        parts[1].trim().parse::<u8>(),
                        parts[2].trim().parse::<u8>(),
                    ) {
                        if !is_grayscale_rgb(r, g, b) {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

fn has_dark_background(obj: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = obj.walk();
    obj.children(&mut cursor).any(|child| {
        if child.kind() != "pair" {
            return false;
        }
        let k = child
            .child_by_field_name("key")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        if k != "backgroundColor" && k != "background" {
            return false;
        }
        let v = child
            .child_by_field_name("value")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        let clean = v.trim_matches(|c| c == '"' || c == '\'');
        is_dark_color_value(clean)
    })
}

fn has_colored_box_shadow(obj: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = obj.walk();
    obj.children(&mut cursor).any(|child| {
        if child.kind() != "pair" {
            return false;
        }
        let k = child
            .child_by_field_name("key")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        if k != "boxShadow" {
            return false;
        }
        let v = child
            .child_by_field_name("value")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        let clean = v.trim_matches(|c| c == '"' || c == '\'');
        shadow_has_chroma(clean)
    })
}

crate::ast_check! { on ["jsx_attribute"] prefilter = ["boxShadow"] => |node, source, ctx, diagnostics|
    let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if attr_name != "style" {
        return;
    }

    let Some(value_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    let obj = if value_node.kind() == "jsx_expression" {
        match value_node.named_child(0) {
            Some(o) => o,
            None => return,
        }
    } else {
        return;
    };
    if obj.kind() != "object" {
        return;
    }

    if !has_dark_background(obj, source) || !has_colored_box_shadow(obj, source) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "Colored glow shadow on a dark background — prefer subtle neutral shadows.".into(),
        severity: Severity::Warning,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
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
