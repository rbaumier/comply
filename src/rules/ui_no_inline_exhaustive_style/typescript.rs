//! Flags JSX `style={{...}}` with > 8 properties.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if attr_name != "style" {
        return;
    }

    let Some(value_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    // style={...} → jsx_expression wrapping the object
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

    let mut cursor = obj.walk();
    let prop_count = obj
        .children(&mut cursor)
        .filter(|c| c.kind() == "pair" || c.kind() == "shorthand_property_identifier")
        .count();

    let max_properties = ctx.config.threshold("ui-no-inline-exhaustive-style", "max_properties", ctx.lang);
    if prop_count <= max_properties {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Inline `style` has {prop_count} properties — extract to a CSS class or styled component."
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
    fn flags_exhaustive_inline_style() {
        let src = r#"<div style={{
            color: 'red',
            fontSize: 14,
            fontWeight: 'bold',
            margin: 0,
            padding: 10,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            border: '1px solid',
        }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_few_inline_styles() {
        assert!(run(r#"<div style={{ color: 'red', fontSize: 14 }} />"#).is_empty());
    }

    #[test]
    fn allows_exactly_8() {
        let src = r#"<div style={{
            color: 'red',
            fontSize: 14,
            fontWeight: 'bold',
            margin: 0,
            padding: 10,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
        }} />"#;
        assert!(run(src).is_empty());
    }
}
