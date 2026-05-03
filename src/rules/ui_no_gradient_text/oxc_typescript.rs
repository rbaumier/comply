//! ui-no-gradient-text OxcCheck backend — flag inline styles combining
//! `backgroundClip: 'text'` with a gradient background.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeValue, JSXExpression, ObjectPropertyKind,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["backgroundClip"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(el) = node.kind() else { return };

        for attr_item in &el.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let name = match &attr.name {
                oxc_ast::ast::JSXAttributeName::Identifier(id) => id.name.as_str(),
                _ => continue,
            };
            if name != "style" {
                continue;
            }
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) =
                &container.expression
            else {
                continue;
            };

            let has_clip = has_pair(obj, &["backgroundClip", "WebkitBackgroundClip"], "text");
            let has_gradient = has_gradient_pair(obj, ctx.source);

            if has_clip && has_gradient {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span().start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Gradient text via `backgroundClip: 'text'` is often inaccessible — use a solid color.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

fn has_pair(
    obj: &oxc_ast::ast::ObjectExpression,
    keys: &[&str],
    value_substr: &str,
) -> bool {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        let key_name = match &p.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if !keys.contains(&key_name) {
            continue;
        }
        let val = match &p.value {
            Expression::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if val.contains(value_substr) {
            return true;
        }
    }
    false
}

fn has_gradient_pair(obj: &oxc_ast::ast::ObjectExpression, _source: &str) -> bool {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        let key_name = match &p.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if key_name != "background" && key_name != "backgroundImage" {
            continue;
        }
        let val = match &p.value {
            Expression::StringLiteral(s) => s.value.as_str(),
            Expression::TemplateLiteral(t) => {
                // Check quasis for "gradient"
                for quasi in &t.quasis {
                    if quasi.value.raw.as_str().contains("gradient") {
                        return true;
                    }
                }
                continue;
            }
            _ => continue,
        };
        if val.contains("gradient") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
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
