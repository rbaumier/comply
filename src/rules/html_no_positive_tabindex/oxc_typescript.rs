//! html-no-positive-tabindex — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue};
use std::sync::Arc;

pub struct Check;

fn is_positive_value(val: &JSXAttributeValue) -> bool {
    match val {
        JSXAttributeValue::StringLiteral(s) => {
            if let Ok(n) = s.value.as_str().trim().parse::<i32>() {
                return n > 0;
            }
            false
        }
        JSXAttributeValue::ExpressionContainer(container) => {
            let Some(expr) = container.expression.as_expression() else {
                return false;
            };
            match expr {
                Expression::NumericLiteral(n) => n.value > 0.0,
                Expression::UnaryExpression(u) => {
                    // Handle negative: -{N}
                    if matches!(u.operator, oxc_ast::ast::UnaryOperator::UnaryNegation)
                        && let Expression::NumericLiteral(n) = &u.argument {
                            return (-n.value) > 0.0;
                        }
                    false
                }
                _ => false,
            }
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["tabindex"])
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
            let JSXAttributeName::Identifier(name) = &attr.name else { continue };
            if name.name.as_str() != "tabindex" {
                continue;
            }
            let Some(val) = &attr.value else { continue };
            if !is_positive_value(val) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, attr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`tabindex` must not be positive — use `0` or `-1`.".into(),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_positive_tabindex_string() {
        assert_eq!(run(r#"const x = <div tabindex="5" />;"#).len(), 1);
    }


    #[test]
    fn flags_positive_tabindex_expr() {
        assert_eq!(run(r#"const x = <div tabindex={3} />;"#).len(), 1);
    }


    #[test]
    fn allows_zero() {
        assert!(run(r#"const x = <div tabindex="0" />;"#).is_empty());
    }


    #[test]
    fn allows_negative() {
        assert!(run(r#"const x = <div tabindex={-1} />;"#).is_empty());
    }
}
