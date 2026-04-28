//! Flags inline `filter` styles whose value contains `blur(Npx)` with
//! N > 20. Large blur radii are expensive on the GPU; the cost scales with
//! both the radius and the painted layer size.

use crate::diagnostic::{Diagnostic, Severity};

const BLUR_THRESHOLD_PX: f64 = 20.0;

fn is_in_style_jsx_attribute(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(obj) = node.parent() else { return false };
    if obj.kind() != "object" { return false; }
    let Some(jsx_expr) = obj.parent() else { return false };
    if jsx_expr.kind() != "jsx_expression" { return false; }
    let Some(jsx_attr) = jsx_expr.parent() else { return false };
    if jsx_attr.kind() != "jsx_attribute" { return false; }
    crate::rules::jsx::jsx_attribute_name(jsx_attr, source) == Some("style")
}

/// Parse the largest blur radius (in px) referenced inside `value`. Looks
/// for `blur(<number>px)` substrings; returns `None` if nothing matches.
fn max_blur_px(value: &str) -> Option<f64> {
    let lower = value.to_ascii_lowercase();
    let mut max: Option<f64> = None;
    for (i, _) in lower.match_indices("blur(") {
        let rest = &lower[i + 5..];
        let Some(end) = rest.find(')') else { continue };
        let inner = rest[..end].trim();
        let num_part = inner.strip_suffix("px").unwrap_or(inner).trim();
        let Ok(n) = num_part.parse::<f64>() else { continue };
        max = Some(max.map_or(n, |m| m.max(n)));
    }
    max
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if !is_in_style_jsx_attribute(node, source) {
        return;
    }

    let Some(key_node) = node.child_by_field_name("key") else { return };
    let Ok(key_text) = key_node.utf8_text(source) else { return };
    let key = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key != "filter" && key != "backdropFilter" && key != "WebkitBackdropFilter" {
        return;
    }

    let Some(value_node) = node.child_by_field_name("value") else { return };
    let Ok(value_raw) = value_node.utf8_text(source) else { return };
    let value = value_raw.trim_matches(|c| c == '"' || c == '\'');

    let Some(radius) = max_blur_px(value) else { return };
    if radius <= BLUR_THRESHOLD_PX {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`blur({radius}px)` exceeds {BLUR_THRESHOLD_PX}px — cost escalates with radius and \
             layer size, can exhaust GPU memory on mobile."
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
    fn flags_blur_30px() {
        assert_eq!(run(r#"<div style={{ filter: 'blur(30px)' }} />"#).len(), 1);
    }

    #[test]
    fn flags_blur_50px_backdrop() {
        assert_eq!(
            run(r#"<div style={{ backdropFilter: 'blur(50px)' }} />"#).len(),
            1
        );
    }

    #[test]
    fn flags_blur_25px() {
        assert_eq!(run(r#"<div style={{ filter: 'blur(25px)' }} />"#).len(), 1);
    }

    #[test]
    fn allows_blur_10px() {
        assert!(run(r#"<div style={{ filter: 'blur(10px)' }} />"#).is_empty());
    }

    #[test]
    fn allows_blur_at_threshold() {
        assert!(run(r#"<div style={{ filter: 'blur(20px)' }} />"#).is_empty());
    }

    #[test]
    fn allows_non_blur_filter() {
        assert!(
            run(r#"<div style={{ filter: 'brightness(1.2) contrast(1.1)' }} />"#).is_empty()
        );
    }

    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const config = { filter: 'blur(50px)' };"#).is_empty());
    }
}
