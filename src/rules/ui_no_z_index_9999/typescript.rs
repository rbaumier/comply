//! Flags `zIndex` values > 100 in inline style objects.

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

crate::ast_check! { on ["pair"] prefilter = ["zIndex"] => |node, source, ctx, diagnostics|
    if !is_in_style_jsx_attribute(node, source) {
        return;
    }

    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).ok().unwrap_or("");
    if key_text != "zIndex" && key_text != "\"zIndex\"" && key_text != "'zIndex'" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "number" {
        return;
    }
    let Ok(num_str) = value.utf8_text(source) else { return };
    let Ok(num) = num_str.parse::<i64>() else { return };

    let max_z = ctx.config.threshold("ui-no-z-index-9999", "max", ctx.lang) as i64;
    if num <= max_z {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`zIndex: {num}` — values above {max_z} indicate a z-index arms race."
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
    fn flags_high_z_index() {
        assert_eq!(run(r#"<div style={{ zIndex: 9999 }} />"#).len(), 1);
    }

    #[test]
    fn flags_z_index_999() {
        assert_eq!(run(r#"<div style={{ zIndex: 999 }} />"#).len(), 1);
    }

    #[test]
    fn allows_z_index_50() {
        assert!(run(r#"<div style={{ zIndex: 50 }} />"#).is_empty());
    }

    #[test]
    fn allows_z_index_100() {
        assert!(run(r#"<div style={{ zIndex: 100 }} />"#).is_empty());
    }

    #[test]
    fn allows_non_z_index_property() {
        assert!(run(r#"<div style={{ fontSize: 9999 }} />"#).is_empty());
    }

    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const theme = { zIndex: { modal: 1000, tooltip: 1100 } };"#).is_empty());
    }
}
