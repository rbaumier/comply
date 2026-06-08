//! Flags inline `letterSpacing` string values expressed in `em` whose
//! numeric part exceeds 0.05. Other units (e.g. `px`) are ignored.

use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { on ["pair"] prefilter = ["letterSpacing"] => |node, source, ctx, diagnostics|
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

    let max_spacing = ctx.config.float("ui-no-wide-letter-spacing", "max_letter_spacing_em", ctx.lang);
    if num <= max_spacing {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`letterSpacing: {raw}` — values above {max_spacing}em hurt \
             readability."
        ),
        severity: Severity::Warning,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
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
