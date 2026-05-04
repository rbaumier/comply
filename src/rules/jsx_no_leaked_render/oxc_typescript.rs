use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True if the identifier (last dot-segment) starts with a boolean-prefix.
fn likely_boolean(name: &str) -> bool {
    let segment = name.rsplit('.').next().unwrap_or(name);
    let lower = segment.to_lowercase();
    const PREFIXES: &[&str] = &[
        "is", "has", "should", "can", "will", "did", "show", "hide", "enable", "disable",
        "visible", "active", "open", "loading", "loaded",
    ];
    PREFIXES.iter().any(|p| lower.starts_with(p))
}

/// True if the expression is a JSX element/fragment.
fn is_jsx(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::JSXElement(_) | Expression::JSXFragment(_)
    )
}

/// True if the expression is a boolean coercion (`!!x`).
fn is_double_bang(expr: &Expression) -> bool {
    if let Expression::UnaryExpression(outer) = expr
        && outer.operator == oxc_ast::ast::UnaryOperator::LogicalNot
            && let Expression::UnaryExpression(inner) = &outer.argument {
                return inner.operator == oxc_ast::ast::UnaryOperator::LogicalNot;
            }
    false
}

/// True if the expression is a comparison that produces a boolean.
fn is_comparison(expr: &Expression) -> bool {
    if let Expression::BinaryExpression(bin) = expr {
        use oxc_ast::ast::BinaryOperator;
        matches!(
            bin.operator,
            BinaryOperator::LessThan
                | BinaryOperator::GreaterThan
                | BinaryOperator::LessEqualThan
                | BinaryOperator::GreaterEqualThan
                | BinaryOperator::Equality
                | BinaryOperator::StrictEquality
                | BinaryOperator::Inequality
                | BinaryOperator::StrictInequality
        )
    } else {
        false
    }
}

/// Get the source text for a span.
fn span_text(source: &str, span: oxc_span::Span) -> &str {
    &source[span.start as usize..span.end as usize]
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXExpressionContainer]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXExpressionContainer(container) = node.kind() else {
            return;
        };
        let oxc_ast::ast::JSXExpression::LogicalExpression(logical) = &container.expression else {
            return;
        };
        if logical.operator != LogicalOperator::And {
            return;
        }
        // Right side must contain JSX.
        if !is_jsx(&logical.right) {
            return;
        }
        let left = &logical.left;
        // Skip `!!x`.
        if is_double_bang(left) {
            return;
        }
        // Skip comparisons.
        if is_comparison(left) {
            return;
        }
        // Skip boolean-prefixed identifiers.
        let left_text = span_text(ctx.source, left.span()).trim();
        if likely_boolean(left_text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, logical.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Potential leaked render — numeric/string value with `&&` renders \
                      falsy value (`0`, `\"\"`) instead of nothing."
                .into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_count_and_jsx() {
        let src = "const x = <div>{count && <Component />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_length_and_jsx() {
        let src = "const x = <div>{items.length && <List />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_double_bang() {
        let src = "const x = <div>{!!count && <Component />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_comparison() {
        let src = "const x = <div>{count > 0 && <Component />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_boolean_prefix() {
        let src = "const x = <div>{isReady && <Component />}</div>;";
        assert!(run_on(src).is_empty());
    }
}
