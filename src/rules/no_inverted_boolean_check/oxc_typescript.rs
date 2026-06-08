//! no-inverted-boolean-check oxc backend — flag `!a === b` patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, UnaryOperator};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        if !matches!(
            bin.operator,
            BinaryOperator::StrictEquality
                | BinaryOperator::StrictInequality
                | BinaryOperator::Equality
                | BinaryOperator::Inequality
        ) {
            return;
        }

        // Left operand must be a unary `!` expression.
        let Expression::UnaryExpression(unary) = &bin.left else { return };
        if unary.operator != UnaryOperator::LogicalNot {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`!a === b` negates `a` before comparing — use `a !== b` or `!(a === b)`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_not_a_strict_equals_b() {
        assert_eq!(run_on("if (!a === b) {}").len(), 1);
    }


    #[test]
    fn flags_not_a_strict_not_equals_b() {
        assert_eq!(run_on("if (!a !== b) {}").len(), 1);
    }


    #[test]
    fn flags_with_member_access() {
        assert_eq!(run_on("if (!foo.bar === baz) {}").len(), 1);
    }


    #[test]
    fn allows_normal_comparison() {
        assert!(run_on("if (a === b) {}").is_empty());
    }


    #[test]
    fn allows_negated_result() {
        assert!(run_on("if (!(a === b)) {}").is_empty());
    }


    #[test]
    fn allows_not_equals_operator() {
        assert!(run_on("if (a !== b) {}").is_empty());
    }
}
