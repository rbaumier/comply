//! ui-no-justified-text OXC backend — flag `textAlign: 'justify'`
//! without `hyphens: 'auto'` in JSX style attributes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
    ObjectPropertyKind, PropertyKey,
};
use std::sync::Arc;

pub struct Check;

fn property_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

fn string_value<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["textAlign"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        // Find the `style` attribute
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

            // Value must be a JSX expression containing an object: style={{ ... }}
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value
            else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) =
                &container.expression
            else {
                continue;
            };

            // Look for textAlign: 'justify'
            let mut text_align_justify_span = None;
            let mut has_hyphens_auto = false;

            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(p) = prop else {
                    continue;
                };
                let Some(key_name) = property_key_name(&p.key) else {
                    continue;
                };
                if key_name == "textAlign" {
                    if let Some(val) = string_value(&p.value) {
                        if val == "justify" {
                            text_align_justify_span = Some(p.span);
                        }
                    }
                } else if key_name == "hyphens" {
                    if let Some(val) = string_value(&p.value) {
                        if val == "auto" {
                            has_hyphens_auto = true;
                        }
                    }
                }
            }

            if let Some(span) = text_align_justify_span {
                if !has_hyphens_auto {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message:
                            "`textAlign: 'justify'` without `hyphens: 'auto'` — produces rivers of whitespace."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_justify_without_hyphens() {
        assert_eq!(run(r#"<p style={{ textAlign: 'justify' }} />"#).len(), 1);
    }

    #[test]
    fn allows_justify_with_hyphens() {
        assert!(
            run(r#"<p style={{ textAlign: 'justify', hyphens: 'auto' }} />"#).is_empty()
        );
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
