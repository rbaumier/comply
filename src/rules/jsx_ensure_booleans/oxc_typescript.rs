use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::boolean_prefix::has_boolean_prefix;
use oxc_ast::ast::Expression;
use std::sync::Arc;

const BOOLEAN_PREFIXES: &[&str] = &[
    "is", "has", "should", "can", "will", "did", "show", "hide", "enable", "disable", "visible",
    "active", "open", "loading", "loaded", "allow", "need", "must",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LogicalExpression(logic) = node.kind() else {
            return;
        };

        if logic.operator != oxc_ast::ast::LogicalOperator::And {
            return;
        }

        // Right side must be JSX
        if !is_jsx_expression(&logic.right) {
            return;
        }

        // Left side must NOT be an obvious boolean
        if is_boolean_expression(&logic.left, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, logic.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Left-hand side of `&&` before JSX is not a boolean — coerce with `!!` or use a comparison to avoid rendering `0`/`\"\"`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_jsx_expression(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::JSXElement(_) | Expression::JSXFragment(_)
    )
}

fn is_boolean_expression(expr: &Expression, source: &str) -> bool {
    match expr {
        Expression::BooleanLiteral(_) => true,
        Expression::UnaryExpression(unary) => {
            unary.operator == oxc_ast::ast::UnaryOperator::LogicalNot
        }
        Expression::BinaryExpression(bin) => {
            use oxc_ast::ast::BinaryOperator;
            matches!(
                bin.operator,
                BinaryOperator::Equality
                    | BinaryOperator::StrictEquality
                    | BinaryOperator::Inequality
                    | BinaryOperator::StrictInequality
                    | BinaryOperator::LessThan
                    | BinaryOperator::LessEqualThan
                    | BinaryOperator::GreaterThan
                    | BinaryOperator::GreaterEqualThan
                    | BinaryOperator::In
                    | BinaryOperator::Instanceof
            )
        }
        // A call to a boolean-prefixed function (`isExpanded(item)`, `hasFoo()`)
        // returns a boolean by naming convention, so `expr && <JSX/>` cannot leak
        // `0`/`""`. Uses the same camelCase-boundary predicate as the sibling
        // `react-no-and-conditional-jsx` to keep the two `&&`-guard rules in parity.
        Expression::CallExpression(call) => {
            matches!(&call.callee, Expression::Identifier(id) if has_boolean_prefix(id.name.as_str()))
        }
        Expression::Identifier(ident) => looks_like_boolean_identifier(ident.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            // `foo.value` is a Vue `Ref`/`ComputedRef` unwrap: the booleanness
            // belongs to the underlying object (`showText.value` → `showText`),
            // so delegate the boolean-name check to `member.object`. This also
            // resolves nested refs (`virtualConfig.isVirtualScroll.value`).
            if member.property.name.as_str() == "value" {
                is_boolean_expression(&member.object, source)
            } else {
                looks_like_boolean_identifier(member.property.name.as_str())
            }
        }
        Expression::ComputedMemberExpression(_) => {
            let span = oxc_span::GetSpan::span(expr);
            let text = &source[span.start as usize..span.end as usize];
            let segment = text.rsplit('.').next().unwrap_or(text);
            looks_like_boolean_identifier(segment)
        }
        Expression::ParenthesizedExpression(paren) => {
            is_boolean_expression(&paren.expression, source)
        }
        _ => false,
    }
}

fn looks_like_boolean_identifier(name: &str) -> bool {
    let lower = name.to_lowercase();
    BOOLEAN_PREFIXES.iter().any(|p| lower.starts_with(p))
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

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_bare_identifier() {
        let src = "const x = <div>{items && <List />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_length_access() {
        let src = "const x = <div>{items.length && <List />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_double_bang_coercion() {
        let src = "const x = <div>{!!items && <List />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_comparison() {
        let src = "const x = <div>{items.length > 0 && <List />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_boolean_identifier() {
        let src = "const x = <div>{isReady && <List />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_negation() {
        let src = "const x = <div>{!error && <Success />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_and_without_jsx_rhs() {
        let src = "const v = a && b;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_boolean_prefixed_call() {
        // #7324: a call to a boolean-prefixed function returns `boolean`, so
        // `expr && <JSX/>` cannot leak `0`/`""`.
        assert!(run_on("const x = isExpanded(item) && <div>hi</div>;").is_empty());
        assert!(run_on("const y = hasFoo() && <div>hi</div>;").is_empty());
        assert!(run_on("const z = shouldBar() && <div>hi</div>;").is_empty());
    }

    #[test]
    fn flags_non_boolean_prefixed_call() {
        // A non-boolean-prefixed call can return a number/string and still leak.
        assert_eq!(run_on("const a = getCount() && <div>hi</div>;").len(), 1);
    }

    #[test]
    fn flags_number_member_before_jsx() {
        let src = "const b = items.length && <div>hi</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_boolean_named_vue_ref_unwrap() {
        // #7378: a Vue `Ref`/`ComputedRef` is read through `.value`; the
        // booleanness belongs to the underlying object, so a boolean-named base
        // (`showText`, `isVirtualScroll`) cannot leak `0`/`""`.
        assert!(run_on("const t = showText.value && <div>hi</div>;").is_empty());
        assert!(
            run_on("const u = virtualConfig.isVirtualScroll.value && <div>hi</div>;").is_empty()
        );
    }

    #[test]
    fn flags_non_boolean_named_vue_ref_unwrap() {
        // A `.value` unwrap whose base is not boolean-named can still hold a
        // number/string and leak `0`/`""`.
        assert_eq!(run_on("const a = items.value && <div>hi</div>;").len(), 1);
        assert_eq!(run_on("const b = count.value && <div>hi</div>;").len(), 1);
    }

    #[test]
    fn allows_boolean_prefixed_property() {
        let src = "const c = props.showText && <div>hi</div>;";
        assert!(run_on(src).is_empty());
    }
}
