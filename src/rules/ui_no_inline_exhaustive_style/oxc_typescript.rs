use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
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
                severity: Severity::Warning,
                span: None,
            });
            return;
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
