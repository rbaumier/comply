//! Flags inline `transition` / `transitionProperty` values that target
//! layout-triggering CSS properties (width, height, top, left, right,
//! bottom, margin, padding, border). These force layout recalculation
//! every frame; prefer `transform`, `opacity`, `color`, `background`,
//! or `filter`.

use crate::diagnostic::{Diagnostic, Severity};

const TIMING_KEYS: &[&str] = &["transition", "transitionProperty"];

/// Layout-triggering property tokens we look for inside the value string.
/// Matched as whole words against `[a-z-]+` tokens.
const LAYOUT_PROPERTIES: &[&str] = &[
    "width",
    "height",
    "min-width",
    "max-width",
    "min-height",
    "max-height",
    "top",
    "left",
    "right",
    "bottom",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border",
    "border-width",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
];

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

/// Find the first layout-triggering property name referenced in `value`.
/// Tokenizes on non-`[a-z-]` characters so e.g. `width 0.3s` matches
/// `width`, but `viewport-width-helper` does not match `width`.
fn first_layout_property(value: &str) -> Option<&'static str> {
    let lower = value.to_ascii_lowercase();
    for token in lower.split(|c: char| !c.is_ascii_lowercase() && c != '-') {
        if token.is_empty() {
            continue;
        }
        for &prop in LAYOUT_PROPERTIES {
            if token == prop {
                return Some(prop);
            }
        }
    }
    None
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if !is_in_style_jsx_attribute(node, source) {
        return;
    }

    let Some(key_node) = node.child_by_field_name("key") else { return };
    let Ok(key_text) = key_node.utf8_text(source) else { return };
    let key = key_text.trim_matches(|c| c == '"' || c == '\'');
    if !TIMING_KEYS.contains(&key) {
        return;
    }

    let Some(value_node) = node.child_by_field_name("value") else { return };
    let Ok(value_raw) = value_node.utf8_text(source) else { return };
    let value = value_raw.trim_matches(|c| c == '"' || c == '\'');

    let Some(prop) = first_layout_property(value) else { return };

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Animating layout property `{prop}` triggers layout recalculation every frame — \
             prefer `transform`, `opacity`, `color`, `background`, or `filter`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_transition_width() {
        let src = r#"<div style={{ transition: 'width 0.3s ease' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_transition_height() {
        let src = r#"<div style={{ transition: 'height 200ms linear' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_transition_property_padding() {
        let src = r#"<div style={{ transitionProperty: 'padding' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_transition_margin_top() {
        let src = r#"<div style={{ transition: 'margin-top 0.2s' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_transform_transition() {
        let src = r#"<div style={{ transition: 'transform 0.3s ease' }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_opacity_transition() {
        let src = r#"<div style={{ transition: 'opacity 0.2s' }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_color_and_background() {
        let src = r#"<div style={{ transition: 'color 0.2s, background 0.3s' }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_filter_transition() {
        let src = r#"<div style={{ transitionProperty: 'filter' }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_style_object() {
        let src = r#"const config = { transition: 'width 0.3s ease' };"#;
        assert!(run(src).is_empty());
    }
}
