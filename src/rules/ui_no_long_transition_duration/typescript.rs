//! Flags inline `transitionDuration` / `animationDuration` string values
//! exceeding 1 second (parsed from `ms` or `s` suffixes).

use crate::diagnostic::{Diagnostic, Severity};

const MS_THRESHOLD: f64 = 1000.0;
const S_THRESHOLD: f64 = 1.0;

/// Parse a CSS duration literal. Returns the value in milliseconds when the
/// duration exceeds the configured threshold, or `None` otherwise.
fn parse_excessive_duration(raw: &str) -> Option<f64> {
    let cleaned = raw.trim_matches(|c| c == '"' || c == '\'').trim();
    if let Some(stripped) = cleaned.strip_suffix("ms") {
        let num = stripped.trim().parse::<f64>().ok()?;
        if num > MS_THRESHOLD {
            return Some(num);
        }
        return None;
    }
    if let Some(stripped) = cleaned.strip_suffix('s') {
        let num = stripped.trim().parse::<f64>().ok()?;
        if num > S_THRESHOLD {
            return Some(num * 1000.0);
        }
    }
    None
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

crate::ast_check! { on ["pair"] prefilter = ["transitionDuration", "animationDuration"] => |node, source, ctx, diagnostics|
    if !is_in_style_jsx_attribute(node, source) {
        return;
    }

    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).ok().unwrap_or("");
    let key_clean = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key_clean != "transitionDuration" && key_clean != "animationDuration" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "string" {
        return;
    }
    let Ok(raw) = value.utf8_text(source) else { return };
    let Some(ms) = parse_excessive_duration(raw) else { return };

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`{key_clean}: {raw}` ({ms}ms) — durations above 1s feel sluggish and block \
             interaction."
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
    fn flags_2000ms_transition() {
        assert_eq!(run(r#"<div style={{ transitionDuration: '2000ms' }} />"#).len(), 1);
    }

    #[test]
    fn flags_3s_animation() {
        assert_eq!(run(r#"<div style={{ animationDuration: '3s' }} />"#).len(), 1);
    }

    #[test]
    fn allows_300ms_transition() {
        assert!(run(r#"<div style={{ transitionDuration: '300ms' }} />"#).is_empty());
    }

    #[test]
    fn allows_half_second_transition() {
        assert!(run(r#"<div style={{ transitionDuration: '0.5s' }} />"#).is_empty());
    }

    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const config = { transitionDuration: '3s' };"#).is_empty());
    }
}
