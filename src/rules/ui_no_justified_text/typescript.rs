//! Flags `textAlign: 'justify'` in inline style objects when no sibling
//! `hyphens: 'auto'` is present.

use crate::diagnostic::{Diagnostic, Severity};

fn key_text(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let key = node.child_by_field_name("key")?;
    let raw = key.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\'').to_string())
}

fn value_text(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let value = node.child_by_field_name("value")?;
    let raw = value.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\'').to_string())
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

    let Some(key) = key_text(node, source) else { return };
    if key != "textAlign" {
        return;
    }
    let Some(value) = value_text(node, source) else { return };
    if value != "justify" {
        return;
    }

    // Walk sibling pairs in the parent object — accept if any sibling sets
    // `hyphens: 'auto'`.
    let Some(parent) = node.parent() else { return };
    let mut cursor = parent.walk();
    let mut has_hyphens = false;
    for child in parent.named_children(&mut cursor) {
        if child.id() == node.id() || child.kind() != "pair" {
            continue;
        }
        let Some(sib_key) = key_text(child, source) else { continue };
        if sib_key != "hyphens" {
            continue;
        }
        let Some(sib_value) = value_text(child, source) else { continue };
        if sib_value == "auto" {
            has_hyphens = true;
            break;
        }
    }

    if has_hyphens {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "`textAlign: 'justify'` without `hyphens: 'auto'` — produces rivers of \
                  whitespace.".into(),
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
    fn flags_justify_without_hyphens() {
        assert_eq!(run(r#"<p style={{ textAlign: 'justify' }} />"#).len(), 1);
    }

    #[test]
    fn allows_justify_with_hyphens() {
        assert!(run(r#"<p style={{ textAlign: 'justify', hyphens: 'auto' }} />"#).is_empty());
    }

    #[test]
    fn allows_center_align() {
        assert!(run(r#"<p style={{ textAlign: 'center' }} />"#).is_empty());
    }

    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const config = { textAlign: 'justify' };"#).is_empty());
    }
}
