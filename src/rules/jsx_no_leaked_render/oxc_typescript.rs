use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True if the identifier starts with a boolean-prefix. A Vue `Ref`/`ComputedRef`
/// is read through `.value`, so when the last dot-segment is `value` the
/// booleanness belongs to the underlying object segment (`showText.value` →
/// `showText`, `virtualConfig.isVirtualScroll.value` → `isVirtualScroll`) — check
/// that segment instead.
fn likely_boolean(name: &str) -> bool {
    let mut segments = name.rsplit('.');
    let last = segments.next().unwrap_or(name);
    let segment = if last == "value" {
        segments.next().unwrap_or(last)
    } else {
        last
    };
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

/// True when `expr` is a logical-NOT (`!x`, `!!x`, …). A `!` unary always
/// evaluates to a primitive `boolean`, so `{!x && <JSX/>}` renders `false`
/// (nothing) and can never leak a falsy `0`/`""` into the DOM.
fn is_logical_not(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::UnaryExpression(u)
            if u.operator == oxc_ast::ast::UnaryOperator::LogicalNot
    )
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
        // Skip a logical-NOT guard (`!x`, `!!x`) — it always yields a boolean.
        if is_logical_not(left) {
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
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
    fn allows_single_bang() {
        let src = "const a = <div>{!isCloud && <SecurityTip />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_bang_on_optional_length() {
        let src = "const b = <div>{!activeSurveys?.length && <p>-</p>}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_double_bang() {
        let src = "const c = <div>{!!count && <X />}</div>;";
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

    #[test]
    fn allows_boolean_ref_read_through_value() {
        // A Vue `Ref`/`ComputedRef` is read through `.value`; the booleanness
        // belongs to the underlying object segment (`showText`), so the unwrap
        // must not flag.
        let src = "const t = () => <div>{showText.value && <span>hi</span>}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_nested_boolean_ref_read_through_value() {
        let src =
            "const u = () => <div>{virtualConfig.isVirtualScroll.value && <span>hi</span>}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_non_boolean_ref_read_through_value() {
        // A `.value` unwrap whose base is not boolean-named can still hold a
        // number/string, so it must still flag.
        let a = "const a = () => <div>{items.value && <span>hi</span>}</div>;";
        assert_eq!(run_on(a).len(), 1);
        let b = "const b = () => <div>{count.value && <span>hi</span>}</div>;";
        assert_eq!(run_on(b).len(), 1);
    }
}
