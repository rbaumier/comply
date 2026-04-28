//! Flags inline `transform` styles whose value contains `scale(0)` or
//! `scale(0, 0)` — animating from zero scale produces subpixel rendering
//! blur and an unnatural "appears from nowhere" entrance.

use crate::diagnostic::{Diagnostic, Severity};

fn is_in_style_jsx_attribute(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(obj) = node.parent() else { return false };
    if obj.kind() != "object" { return false; }
    let Some(jsx_expr) = obj.parent() else { return false };
    if jsx_expr.kind() != "jsx_expression" { return false; }
    let Some(jsx_attr) = jsx_expr.parent() else { return false };
    if jsx_attr.kind() != "jsx_attribute" { return false; }
    crate::rules::jsx::jsx_attribute_name(jsx_attr, source) == Some("style")
}

/// Detect a `scale(0)` / `scale(0,0)` / `scale(0.0)` call. Looks for
/// `scale(` followed by an all-zero argument list (whitespace, commas,
/// digits-equal-to-zero, decimal points).
fn references_scale_zero(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    for (i, _) in lower.match_indices("scale(") {
        let rest = &lower[i + 6..];
        let Some(end) = rest.find(')') else { continue };
        let inner = rest[..end].trim();
        // All comma-separated args must parse to zero.
        let args: Vec<&str> = inner.split(',').map(str::trim).collect();
        if !args.is_empty() && args.iter().all(|a| a.parse::<f64>().is_ok_and(|n| n == 0.0)) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if !is_in_style_jsx_attribute(node, source) {
        return;
    }

    let Some(key_node) = node.child_by_field_name("key") else { return };
    let Ok(key_text) = key_node.utf8_text(source) else { return };
    let key = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key != "transform" {
        return;
    }

    let Some(value_node) = node.child_by_field_name("value") else { return };
    let Ok(value_raw) = value_node.utf8_text(source) else { return };
    let value = value_raw.trim_matches(|c| c == '"' || c == '\'');

    if !references_scale_zero(value) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "`scale(0)` causes subpixel rendering blur and makes elements appear from \
                  nowhere — use `scale(0.95)` with `opacity: 0` instead.".into(),
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
    fn flags_scale_zero() {
        assert_eq!(run(r#"<div style={{ transform: 'scale(0)' }} />"#).len(), 1);
    }

    #[test]
    fn flags_scale_zero_zero() {
        assert_eq!(
            run(r#"<div style={{ transform: 'scale(0, 0)' }} />"#).len(),
            1
        );
    }

    #[test]
    fn flags_scale_decimal_zero() {
        assert_eq!(
            run(r#"<div style={{ transform: 'scale(0.0)' }} />"#).len(),
            1
        );
    }

    #[test]
    fn allows_scale_one() {
        assert!(run(r#"<div style={{ transform: 'scale(1)' }} />"#).is_empty());
    }

    #[test]
    fn allows_scale_point_nine_five() {
        assert!(run(r#"<div style={{ transform: 'scale(0.95)' }} />"#).is_empty());
    }

    #[test]
    fn allows_translate() {
        assert!(
            run(r#"<div style={{ transform: 'translateX(0)' }} />"#).is_empty()
        );
    }

    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const config = { transform: 'scale(0)' };"#).is_empty());
    }
}
