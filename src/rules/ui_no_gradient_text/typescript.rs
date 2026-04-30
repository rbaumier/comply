//! Flags inline styles combining `backgroundClip: 'text'` with a gradient
//! `background` or `backgroundImage`.

use crate::diagnostic::{Diagnostic, Severity};

fn has_pair_value(obj: tree_sitter::Node, source: &[u8], key: &str, value_substr: &str) -> bool {
    let mut cursor = obj.walk();
    obj.children(&mut cursor).any(|child| {
        if child.kind() != "pair" {
            return false;
        }
        let k = child
            .child_by_field_name("key")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        if k != key {
            return false;
        }
        let v = child
            .child_by_field_name("value")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        v.contains(value_substr)
    })
}

fn has_background_clip_text(obj: tree_sitter::Node, source: &[u8]) -> bool {
    has_pair_value(obj, source, "backgroundClip", "text")
        || has_pair_value(obj, source, "WebkitBackgroundClip", "text")
}

fn has_gradient_background(obj: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = obj.walk();
    obj.children(&mut cursor).any(|child| {
        if child.kind() != "pair" {
            return false;
        }
        let k = child
            .child_by_field_name("key")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        if k != "background" && k != "backgroundImage" {
            return false;
        }
        let Some(val) = child.child_by_field_name("value") else {
            return false;
        };
        if val.kind() != "string" && val.kind() != "template_string" {
            return false;
        }
        let v = val.utf8_text(source).ok().unwrap_or("");
        v.contains("gradient")
    })
}

crate::ast_check! { on ["jsx_attribute"] prefilter = ["backgroundClip"] => |node, source, ctx, diagnostics|
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

    if !has_background_clip_text(obj, source) || !has_gradient_background(obj, source) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "Gradient text via `backgroundClip: 'text'` is often inaccessible — use a solid color.".into(),
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
    fn flags_gradient_text() {
        let src = r#"<h1 style={{
            background: 'linear-gradient(to right, red, blue)',
            backgroundClip: 'text',
            color: 'transparent',
        }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_webkit_background_clip() {
        let src = r#"<h1 style={{
            backgroundImage: 'linear-gradient(red, blue)',
            WebkitBackgroundClip: 'text',
        }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_background_clip_without_gradient() {
        let src = r#"<h1 style={{
            background: 'red',
            backgroundClip: 'text',
        }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_gradient_without_clip() {
        let src = r#"<div style={{
            background: 'linear-gradient(red, blue)',
        }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_gradient_variable() {
        let src = r#"<h1 style={{
            background: gradient,
            backgroundClip: 'text',
        }} />"#;
        assert!(run(src).is_empty());
    }
}
