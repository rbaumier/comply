//! Flags inline `borderLeft` / `borderRight` (or their `*Width` variants)
//! when the same inline `style={{ ... }}` object also defines a
//! `borderBottom` / `borderBottomWidth` — the tab-indicator pattern.
//!
//! Only triggers inside the value of a JSX `style` attribute; plain
//! configuration objects with the same key names are ignored.

use crate::diagnostic::{Diagnostic, Severity};

const SIDE_KEYS: &[&str] = &[
    "borderLeft",
    "borderRight",
    "borderLeftWidth",
    "borderRightWidth",
];
const BOTTOM_KEYS: &[&str] = &["borderBottom", "borderBottomWidth"];

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

fn pair_key<'a>(pair: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let key = pair.child_by_field_name("key")?;
    let raw = key.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\''))
}

fn object_has_bottom_border(object: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = object.walk();
    object.children(&mut cursor).any(|child| {
        if child.kind() != "pair" {
            return false;
        }
        match pair_key(child, source) {
            Some(name) => BOTTOM_KEYS.contains(&name),
            None => false,
        }
    })
}

crate::ast_check! { on ["pair"] prefilter = ["borderLeft", "borderRight", "borderTop", "borderBottom"] => |node, source, ctx, diagnostics|
    if !is_in_style_jsx_attribute(node, source) {
        return;
    }

    let Some(key_name) = pair_key(node, source) else { return };
    if !SIDE_KEYS.contains(&key_name) {
        return;
    }

    // borderLeftWidth: 0 / '0' / '0px' explicitly removes the border.
    if key_name.ends_with("Width") {
        if let Some(val) = node.child_by_field_name("value") {
            let text = val.utf8_text(source).ok().unwrap_or("");
            let trimmed = text.trim_matches(|c| c == '\'' || c == '"');
            if trimmed == "0" || trimmed == "0px" {
                return;
            }
        }
    }

    let Some(object) = node.parent() else { return };
    if !object_has_bottom_border(object, source) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`{key_name}` alongside a bottom border looks like a tab indicator — drop the side border."
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
    fn flags_border_left_with_border_bottom() {
        let diags = run(
            r#"<div style={{ borderLeft: '1px solid red', borderBottom: '2px solid blue' }} />"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("borderLeft"));
    }

    #[test]
    fn flags_border_right_with_border_bottom() {
        let diags = run(
            r#"<div style={{ borderRight: '1px solid red', borderBottom: '2px solid blue' }} />"#,
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_border_left_width_with_border_bottom_width() {
        let diags = run(r#"<div style={{ borderLeftWidth: 1, borderBottomWidth: 2 }} />"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_both_sides_with_border_bottom() {
        let diags = run(
            r#"<div style={{ borderLeft: '1px solid red', borderRight: '1px solid red', borderBottom: '2px solid blue' }} />"#,
        );
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn allows_border_left_without_bottom() {
        assert!(run(r#"<div style={{ borderLeft: '1px solid red' }} />"#).is_empty());
    }

    #[test]
    fn allows_border_bottom_alone() {
        assert!(run(r#"<div style={{ borderBottom: '2px solid blue' }} />"#).is_empty());
    }

    #[test]
    fn allows_zero_width_side_border() {
        assert!(
            run(r#"<div style={{ borderLeftWidth: 0, borderBottom: '2px solid blue' }} />"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_zero_px_width_side_border() {
        assert!(
            run(r#"<div style={{ borderRightWidth: '0px', borderBottom: '2px solid blue' }} />"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const config = { borderLeft: '1px', borderBottom: '2px' };"#).is_empty());
    }
}
