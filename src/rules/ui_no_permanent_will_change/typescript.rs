//! Flags any inline `willChange` style whose value isn't `'auto'`.
//! Permanent `will-change` wastes GPU memory and defeats the hint's purpose:
//! it's meant to be applied right before an animation and removed after.

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

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if !is_in_style_jsx_attribute(node, source) {
        return;
    }

    let Some(key_node) = node.child_by_field_name("key") else { return };
    let Ok(key_text) = key_node.utf8_text(source) else { return };
    let key = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key != "willChange" {
        return;
    }

    // Allow the explicit "auto" reset.
    if let Some(value_node) = node.child_by_field_name("value") {
        if let Ok(value_raw) = value_node.utf8_text(source) {
            let value = value_raw.trim_matches(|c| c == '"' || c == '\'');
            if value == "auto" {
                return;
            }
        }
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "Permanent `willChange` wastes GPU memory — apply only during active animation \
                  and remove after.".into(),
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
    fn flags_will_change_transform() {
        assert_eq!(run(r#"<div style={{ willChange: 'transform' }} />"#).len(), 1);
    }

    #[test]
    fn flags_will_change_opacity() {
        assert_eq!(run(r#"<div style={{ willChange: 'opacity' }} />"#).len(), 1);
    }

    #[test]
    fn flags_will_change_multiple() {
        assert_eq!(
            run(r#"<div style={{ willChange: 'transform, opacity' }} />"#).len(),
            1
        );
    }

    #[test]
    fn allows_will_change_auto() {
        assert!(run(r#"<div style={{ willChange: 'auto' }} />"#).is_empty());
    }

    #[test]
    fn allows_other_style_keys() {
        assert!(run(r#"<div style={{ transform: 'translateZ(0)' }} />"#).is_empty());
    }

    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const config = { willChange: 'transform' };"#).is_empty());
    }
}
