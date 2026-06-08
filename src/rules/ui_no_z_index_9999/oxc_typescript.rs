//! ui-no-z-index-9999 OXC backend — flag `zIndex` values > threshold in JSX style objects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
    ObjectPropertyKind, PropertyKey,
};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["zIndex"])
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

        // Find `style={...}` attribute.
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
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) = &container.expression else {
                continue;
            };

            let max_z =
                ctx.config.threshold("ui-no-z-index-9999", "max", ctx.lang) as i64;

            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(p) = prop else {
                    continue;
                };
                let key_name = match &p.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    _ => continue,
                };
                if key_name != "zIndex" {
                    continue;
                }
                let Expression::NumericLiteral(num) = &p.value else {
                    continue;
                };
                if (num.value as i64) <= max_z {
                    continue;
                }
                let val = num.value as i64;
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, p.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`zIndex: {val}` — values above {max_z} indicate a z-index arms race."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
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
