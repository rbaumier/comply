//! Flags inline `fontSize` numeric values below 12 (pixels). String values
//! such as `'0.8rem'` are intentionally ignored — only `number` AST nodes
//! are inspected.

use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { on ["pair"] prefilter = ["fontSize"] => |node, source, ctx, diagnostics|
    if !is_in_style_jsx_attribute(node, source) {
        return;
    }

    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).ok().unwrap_or("");
    let key_clean = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key_clean != "fontSize" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "number" {
        return;
    }
    let Ok(num_str) = value.utf8_text(source) else { return };
    let Ok(num) = num_str.parse::<f64>() else { return };

    let min_font = ctx.config.float("ui-no-tiny-text", "min_font_size_px", ctx.lang);
    if num >= min_font {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`fontSize: {num_str}` — values below {min_font}px are too small for \
             comfortable reading."
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
    fn flags_font_size_10() {
        assert_eq!(run(r#"<span style={{ fontSize: 10 }} />"#).len(), 1);
    }

    #[test]
    fn flags_font_size_8() {
        assert_eq!(run(r#"<span style={{ fontSize: 8 }} />"#).len(), 1);
    }

    #[test]
    fn allows_font_size_14() {
        assert!(run(r#"<span style={{ fontSize: 14 }} />"#).is_empty());
    }

    #[test]
    fn allows_font_size_12() {
        assert!(run(r#"<span style={{ fontSize: 12 }} />"#).is_empty());
    }

    #[test]
    fn allows_string_rem_value() {
        assert!(run(r#"<span style={{ fontSize: '0.8rem' }} />"#).is_empty());
    }

    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const config = { fontSize: 10 };"#).is_empty());
    }
}
