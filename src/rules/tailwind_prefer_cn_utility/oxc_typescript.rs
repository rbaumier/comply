//! tailwind-prefer-cn-utility oxc backend — flag `className={...}` whose
//! expression is a bare ternary or string-concatenation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, JSXAttributeValue};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else { return };
        let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else { return };
        if ident.name.as_str() != "className" {
            return;
        }

        let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
            return;
        };
        let Some(inner) = container.expression.as_expression() else {
            return;
        };

        // If the expression text contains cn(, clsx(, or cva(, allow it.
        let val_text = &ctx.source[container.span.start as usize..container.span.end as usize];
        if val_text.contains("cn(") || val_text.contains("clsx(") || val_text.contains("cva(") {
            return;
        }

        let is_flagged = matches!(
            inner,
            Expression::ConditionalExpression(_) | Expression::BinaryExpression(_)
        );
        if !is_flagged {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, inner.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `cn()` or `clsx()` for conditional class names instead of ternaries or concatenation.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_ternary_classname() {
        assert_eq!(run(r#"<div className={x ? 'flex' : 'hidden'} />"#).len(), 1);
    }


    #[test]
    fn allows_cn_utility() {
        assert!(run(r#"<div className={cn('p-4', x && 'flex')} />"#).is_empty());
    }


    #[test]
    fn allows_static_classname() {
        assert!(run(r#"<div className="flex p-4" />"#).is_empty());
    }
}
