//! ui-no-side-tab-border OXC backend — flag `borderLeft`/`borderRight`
//! alongside `borderBottom` in inline JSX style objects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXExpression, ObjectPropertyKind,
    PropertyKey,
};
use std::sync::Arc;

pub struct Check;

const SIDE_KEYS: &[&str] = &[
    "borderLeft",
    "borderRight",
    "borderLeftWidth",
    "borderRightWidth",
];
const BOTTOM_KEYS: &[&str] = &["borderBottom", "borderBottomWidth"];

fn obj_has_bottom_border(obj: &oxc_ast::ast::ObjectExpression) -> bool {
    obj.properties.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(pair) = prop else { return false };
        let key = match &pair.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            _ => return false,
        };
        BOTTOM_KEYS.contains(&key)
    })
}

fn is_zero_value(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(n) => n.value == 0.0,
        Expression::StringLiteral(s) => {
            let trimmed = s.value.as_str().trim();
            trimmed == "0" || trimmed == "0px"
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["borderLeft", "borderRight", "borderBottom"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let JSXAttributeName::Identifier(attr_name) = &attr.name else { continue };
            if attr_name.name.as_str() != "style" {
                continue;
            }

            let Some(ref value) = attr.value else { continue };
            let oxc_ast::ast::JSXAttributeValue::ExpressionContainer(container) = value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) = &container.expression else {
                continue;
            };

            if !obj_has_bottom_border(obj) {
                continue;
            }

            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(pair) = prop else { continue };
                let key = match &pair.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    _ => continue,
                };
                if !SIDE_KEYS.contains(&key) {
                    continue;
                }

                // borderLeftWidth: 0 / '0' / '0px' explicitly removes the border.
                if key.ends_with("Width") && is_zero_value(&pair.value) {
                    continue;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, pair.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{key}` alongside a bottom border looks like a tab indicator \u{2014} drop the side border."
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
    fn flags_border_left_with_border_bottom() {
        let diags = run(
            r#"<div style={{ borderLeft: '1px solid red', borderBottom: '2px solid blue' }} />"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("borderLeft"));
    }


    #[test]
    fn flags_border_right_with_border_bottom() {
        let diags = run(
            r#"<div style={{ borderRight: '1px solid red', borderBottom: '2px solid blue' }} />"#,
        );
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_border_left_width_with_border_bottom_width() {
        let diags = run(r#"<div style={{ borderLeftWidth: 1, borderBottomWidth: 2 }} />"#);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_both_sides_with_border_bottom() {
        let diags = run(
            r#"<div style={{ borderLeft: '1px solid red', borderRight: '1px solid red', borderBottom: '2px solid blue' }} />"#,
        );
        assert_eq!(diags.len(), 2);
    }


    #[test]
    fn allows_border_left_without_bottom() {
        assert!(run(r#"<div style={{ borderLeft: '1px solid red' }} />"#).is_empty());
    }


    #[test]
    fn allows_border_bottom_alone() {
        assert!(run(r#"<div style={{ borderBottom: '2px solid blue' }} />"#).is_empty());
    }


    #[test]
    fn allows_zero_width_side_border() {
        assert!(
            run(r#"<div style={{ borderLeftWidth: 0, borderBottom: '2px solid blue' }} />"#)
                .is_empty()
        );
    }


    #[test]
    fn allows_zero_px_width_side_border() {
        assert!(
            run(r#"<div style={{ borderRightWidth: '0px', borderBottom: '2px solid blue' }} />"#)
                .is_empty()
        );
    }


    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const config = { borderLeft: '1px', borderBottom: '2px' };"#).is_empty());
    }
}
