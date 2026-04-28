//! Flags inline `letterSpacing` string values expressed in `em` whose
//! numeric part exceeds 0.05. Other units (e.g. `px`) are ignored.

use crate::diagnostic::{Diagnostic, Severity};

const LETTER_SPACING_EM_THRESHOLD: f64 = 0.05;

fn parse_em(raw: &str) -> Option<f64> {
    let cleaned = raw.trim_matches(|c| c == '"' || c == '\'').trim();
    let stripped = cleaned.strip_suffix("em")?;
    // Reject `rem` — only bare `em` is in scope here.
    if stripped.ends_with('r') {
        return None;
    }
    stripped.trim().parse::<f64>().ok()
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

    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).ok().unwrap_or("");
    let key_clean = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key_clean != "letterSpacing" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "string" {
        return;
    }
    let Ok(raw) = value.utf8_text(source) else { return };
    let Some(num) = parse_em(raw) else { return };

    if num <= LETTER_SPACING_EM_THRESHOLD {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`letterSpacing: {raw}` — values above {LETTER_SPACING_EM_THRESHOLD}em hurt \
             readability."
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
    fn flags_0_1_em() {
        assert_eq!(run(r#"<p style={{ letterSpacing: '0.1em' }} />"#).len(), 1);
    }

    #[test]
    fn flags_0_2_em() {
        assert_eq!(run(r#"<p style={{ letterSpacing: '0.2em' }} />"#).len(), 1);
    }

    #[test]
    fn allows_0_03_em() {
        assert!(run(r#"<p style={{ letterSpacing: '0.03em' }} />"#).is_empty());
    }

    #[test]
    fn allows_pixel_value() {
        assert!(run(r#"<p style={{ letterSpacing: '2px' }} />"#).is_empty());
    }

    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const config = { letterSpacing: '0.2em' };"#).is_empty());
    }
}
