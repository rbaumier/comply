//! html-no-aria-hidden-body OXC backend — flag `<body aria-hidden="true">` in JSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName, JSXExpression,
};
use std::sync::Arc;

fn is_aria_hidden_true(attr: &oxc_ast::ast::JSXAttribute) -> bool {
    let JSXAttributeName::Identifier(name) = &attr.name else { return false };
    if name.name.as_str() != "aria-hidden" {
        return false;
    }
    let Some(val) = &attr.value else {
        // Shorthand: just `aria-hidden` without a value is truthy.
        return true;
    };
    match val {
        JSXAttributeValue::StringLiteral(lit) => lit.value.as_str() == "true",
        JSXAttributeValue::ExpressionContainer(expr) => {
            if let JSXExpression::BooleanLiteral(b) = &expr.expression {
                return b.value;
            }
            false
        }
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["aria-hidden"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        // Check tag name is "body".
        let JSXElementName::Identifier(tag) = &opening.name else { return };
        if tag.name.as_str() != "body" {
            return;
        }

        for attr_item in opening.attributes.iter() {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            if is_aria_hidden_true(attr) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`aria-hidden=\"true\"` on `<body>` hides the entire page from assistive tech.".into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_body_aria_hidden_true_string() {
        assert_eq!(run(r#"const x = <body aria-hidden="true" />;"#).len(), 1);
    }

    #[test]
    fn flags_body_aria_hidden_expr() {
        assert_eq!(run(r#"const x = <body aria-hidden={true} />;"#).len(), 1);
    }

    #[test]
    fn allows_body_aria_hidden_false() {
        assert!(run(r#"const x = <body aria-hidden="false" />;"#).is_empty());
    }

    #[test]
    fn allows_aria_hidden_on_div() {
        assert!(run(r#"const x = <div aria-hidden="true" />;"#).is_empty());
    }

    #[test]
    fn allows_plain_body() {
        assert!(run(r#"const x = <body />;"#).is_empty());
    }
}
