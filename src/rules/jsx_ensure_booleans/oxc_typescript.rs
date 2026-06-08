use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        Expression::Identifier(ident) => looks_like_boolean_identifier(ident.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            looks_like_boolean_identifier(member.property.name.as_str())
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
}
