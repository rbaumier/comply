//! a11y-no-noninteractive-tabindex oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName, JSXExpression,
    UnaryOperator,
};
use std::sync::Arc;

const NON_INTERACTIVE: &[&str] = &["div", "span", "p", "section"];

pub struct Check;

/// True when `expr` can only ever evaluate to a non-positive `tabIndex`: `-1`,
/// `undefined`, or `null`. A `ConditionalExpression` is non-positive only when
/// both branches are (checked recursively, so nested conditionals are covered);
/// any branch that is `0`, a positive number, or any other expression makes the
/// whole value not allowed.
fn is_non_positive_tabindex_value(expr: &Expression) -> bool {
    // Strip parentheses and TS wrappers (`as`/`satisfies`/`!`) so a branch like
    // `(b ? undefined : null)` is analysed by its inner expression.
    match expr.get_inner_expression() {
        Expression::UnaryExpression(unary) => is_negative_one(unary),
        Expression::Identifier(ident) => ident.name.as_str() == "undefined",
        Expression::NullLiteral(_) => true,
        Expression::ConditionalExpression(cond) => {
            is_non_positive_tabindex_value(&cond.consequent)
                && is_non_positive_tabindex_value(&cond.alternate)
        }
        _ => false,
    }
}

/// Same allowed-value check entered from a JSX `{...}` container. Bridges the
/// distinct `JSXExpression` enum to [`is_non_positive_tabindex_value`]; the
/// `ConditionalExpression` branches are already `Expression`, so the recursion
/// lives in one place.
fn is_non_positive_tabindex_jsx(expr: &JSXExpression) -> bool {
    match expr {
        JSXExpression::UnaryExpression(unary) => is_negative_one(unary),
        JSXExpression::Identifier(ident) => ident.name.as_str() == "undefined",
        JSXExpression::NullLiteral(_) => true,
        JSXExpression::ParenthesizedExpression(paren) => {
            is_non_positive_tabindex_value(&paren.expression)
        }
        JSXExpression::ConditionalExpression(cond) => {
            is_non_positive_tabindex_value(&cond.consequent)
                && is_non_positive_tabindex_value(&cond.alternate)
        }
        _ => false,
    }
}

/// True for the unary expression `-1`.
fn is_negative_one(unary: &oxc_ast::ast::UnaryExpression) -> bool {
    unary.operator == UnaryOperator::UnaryNegation
        && matches!(&unary.argument, Expression::NumericLiteral(num) if num.value == 1.0)
}

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

        // Only flag non-interactive HTML elements.
        let JSXElementName::Identifier(tag_ident) = &opening.name else {
            return;
        };
        let tag = tag_ident.name.as_str();
        if !NON_INTERACTIVE.contains(&tag) {
            return;
        }

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            if name_ident.name.as_str() != "tabIndex" {
                continue;
            }

            // tabIndex is allowed when it can only ever be non-positive: the
            // string `"-1"`, or an expression yielding `-1`/`undefined`/`null`
            // (including conditionals whose every branch is non-positive).
            let is_allowed = match &attr.value {
                Some(JSXAttributeValue::StringLiteral(lit)) => lit.value.as_str() == "-1",
                Some(JSXAttributeValue::ExpressionContainer(container)) => {
                    is_non_positive_tabindex_jsx(&container.expression)
                }
                _ => false,
            };

            if !is_allowed {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, opening.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Non-interactive element `<{tag}>` should not have `tabIndex`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
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

    #[test]
    fn flags_div_with_tabindex_zero() {
        assert_eq!(run(r#"const x = <div tabIndex={0}>Focusable div</div>;"#).len(), 1);
    }

    #[test]
    fn allows_div_with_tabindex_negative_one() {
        assert!(run(r#"const x = <div tabIndex={-1}>Not focusable</div>;"#).is_empty());
    }

    #[test]
    fn allows_div_with_string_tabindex_negative_one() {
        assert!(run(r#"const x = <div tabIndex="-1">Not focusable</div>;"#).is_empty());
    }

    #[test]
    fn allows_button_with_tabindex() {
        assert!(run(r#"const x = <button tabIndex={0}>OK</button>;"#).is_empty());
    }

    #[test]
    fn flags_span_with_tabindex() {
        assert_eq!(run(r#"const x = <span tabIndex={1}>text</span>;"#).len(), 1);
    }

    // Regression for #3985: conditional that only ever yields -1 / undefined.
    #[test]
    fn allows_conditional_neg_one_or_undefined() {
        assert!(
            run(r#"const x = <div role="alert" tabIndex={autoFocus ? -1 : undefined}>alert</div>;"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_conditional_neg_one_or_null() {
        assert!(run(r#"const x = <div tabIndex={cond ? -1 : null}>x</div>;"#).is_empty());
    }

    #[test]
    fn allows_nested_conditional_all_non_positive() {
        assert!(
            run(r#"const x = <div tabIndex={a ? -1 : (b ? undefined : null)}>x</div>;"#).is_empty()
        );
    }

    // Must still fire: a branch that is 0 or positive is a real Tab-order insertion.
    #[test]
    fn flags_conditional_zero_branch() {
        assert_eq!(run(r#"const x = <div tabIndex={cond ? 0 : -1}>x</div>;"#).len(), 1);
    }

    #[test]
    fn flags_conditional_positive_branch() {
        assert_eq!(run(r#"const x = <div tabIndex={cond ? 1 : undefined}>x</div>;"#).len(), 1);
    }
}
