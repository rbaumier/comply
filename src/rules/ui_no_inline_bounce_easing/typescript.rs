//! Flags bounce/elastic easing in inline style timing functions.

use crate::diagnostic::{Diagnostic, Severity};

const BOUNCE_NAMES: &[&str] = &["bounce", "elastic", "wobble", "jiggle", "spring"];

const TIMING_KEYS: &[&str] = &[
    "transition",
    "transitionTimingFunction",
    "animation",
    "animationTimingFunction",
];

const ANIM_NAME_KEYS: &[&str] = &["animation", "animationName"];

fn is_overshoot_cubic_bezier(value: &str) -> bool {
    let Some(start) = value.find("cubic-bezier(") else {
        return false;
    };
    let rest = &value[start + 13..];
    let Some(end) = rest.find(')') else {
        return false;
    };
    let inner = &rest[..end];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 4 {
        return false;
    }
    let y1: f64 = parts[1].trim().parse().unwrap_or(0.0);
    let y2: f64 = parts[3].trim().parse().unwrap_or(0.0);
    !(-0.1..=1.1).contains(&y1) || !(-0.1..=1.1).contains(&y2)
}

fn has_bounce_animation_name(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|token| BOUNCE_NAMES.iter().any(|&name| token.starts_with(name)))
}

fn is_in_style_jsx_attribute(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(obj) = node.parent() else { return false };
    if obj.kind() != "object" { return false; }
    let Some(jsx_expr) = obj.parent() else { return false };
    if jsx_expr.kind() != "jsx_expression" { return false; }
    let Some(jsx_attr) = jsx_expr.parent() else { return false };
    if jsx_attr.kind() != "jsx_attribute" { return false; }
    crate::rules::jsx::jsx_attribute_name(jsx_attr, source) == Some("style")
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if !is_in_style_jsx_attribute(node, source) {
        return;
    }

    let Some(key_node) = node.child_by_field_name("key") else { return };
    let Ok(key) = key_node.utf8_text(source) else { return };

    let Some(value_node) = node.child_by_field_name("value") else { return };
    let Ok(value) = value_node.utf8_text(source) else { return };
    // Strip quotes from string values.
    let value = value.trim_matches(|c| c == '\'' || c == '"');

    let is_timing = TIMING_KEYS.contains(&key);
    let is_anim_name = ANIM_NAME_KEYS.contains(&key);

    if !is_timing && !is_anim_name {
        return;
    }

    let flagged = (is_timing && is_overshoot_cubic_bezier(value))
        || (is_anim_name && has_bounce_animation_name(value));

    if !flagged {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "Bounce/elastic easing — use `ease-out` or a smooth deceleration curve.".into(),
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
    fn flags_overshoot_cubic_bezier() {
        let src = r#"<div style={{ transition: 'all 0.3s cubic-bezier(0.68, -0.55, 0.27, 1.55)' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bounce_animation_name() {
        let src = r#"<div style={{ animationName: 'bounceIn' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_elastic_keyword() {
        let src = r#"<div style={{ animation: '0.5s elastic ease-in' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ease_out() {
        assert!(run(r#"<div style={{ transition: 'all 0.3s ease-out' }} />"#).is_empty());
    }

    #[test]
    fn allows_smooth_cubic_bezier() {
        let src = r#"<div style={{ transitionTimingFunction: 'cubic-bezier(0.16, 1, 0.3, 1)' }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_timing_key() {
        assert!(run(r#"<div style={{ color: 'bounce' }} />"#).is_empty());
    }

    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const config = { transition: 'all 0.3s cubic-bezier(0.68, -0.55, 0.27, 1.55)' };"#).is_empty());
    }

    #[test]
    fn allows_substring_match_spring() {
        assert!(run(r#"<div style={{ animationName: 'mainspringTransition' }} />"#).is_empty());
    }
}
