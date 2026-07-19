use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, file_imports_email_template_library};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            if name.name.as_str() != "style" {
                continue;
            }

            // style={...} — value must be a JSXExpressionContainer wrapping an object.
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) = &container.expression else {
                continue;
            };

            let prop_count = obj.properties.len();
            let max_properties =
                ctx.config
                    .threshold("ui-no-inline-exhaustive-style", "max_properties", ctx.lang);
            if prop_count <= max_properties {
                continue;
            }

            // HTML-email templates (react-email, jsx-email, MJML) must style
            // everything inline — email clients strip `<style>` blocks and
            // external CSS — so extracting to a CSS class is not actionable.
            if file_imports_email_template_library(semantic) {
                return;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, attr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Inline `style` has {prop_count} properties \u{2014} extract to a CSS class or styled component."
                ),
                severity: Severity::Error,
                span: None,
            });
            return;
        }
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    const EXHAUSTIVE_STYLE: &str = r#"<a style={{
        lineHeight: '100%',
        textDecoration: 'none',
        display: 'inline-block',
        maxWidth: '100%',
        msoPaddingAlt: '0px',
        paddingTop: 1,
        paddingRight: 2,
        paddingBottom: 3,
        paddingLeft: 4,
        color: 'red',
    }} />"#;

    #[test]
    fn flags_exhaustive_inline_style_in_ordinary_component() {
        let src = format!("import React from 'react';\nexport const C = () => ({EXHAUSTIVE_STYLE});");
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn allows_exhaustive_inline_style_in_react_email_component() {
        let src = format!(
            "import {{ Button }} from '@react-email/components';\nexport const C = () => ({EXHAUSTIVE_STYLE});"
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn allows_exhaustive_inline_style_in_react_email_subpackage() {
        let src = format!(
            "import {{ Button }} from '@react-email/button';\nexport const C = () => ({EXHAUSTIVE_STYLE});"
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn allows_exhaustive_inline_style_in_jsx_email_component() {
        let src = format!(
            "import {{ Button }} from 'jsx-email';\nexport const C = () => ({EXHAUSTIVE_STYLE});"
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn allows_under_threshold_inline_style() {
        let src = r#"<div style={{ color: 'red', padding: 4 }} />"#;
        assert!(run(src).is_empty());
    }
}
