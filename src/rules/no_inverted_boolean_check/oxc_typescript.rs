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

        // `!!x === y` is boolean coercion, not the `!a === b` precedence bug: a
        // nested `!` on the argument means the author deliberately coerced to a
        // boolean, so skip it.
        if matches!(
            &unary.argument,
            Expression::UnaryExpression(inner) if inner.operator == UnaryOperator::LogicalNot
        ) {
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_not_a_strict_equals_b() {
        assert_eq!(run_on("const r = !a === b;").len(), 1);
    }

    #[test]
    fn flags_not_a_strict_not_equals_b() {
        assert_eq!(run_on("const r = !a !== b;").len(), 1);
    }

    #[test]
    fn allows_double_negation_coercion() {
        assert!(run_on("const r = !!x === y ? x : false;").is_empty());
    }

    #[test]
    fn allows_double_negation_member_coercion() {
        assert!(
            run_on(
                "const r = !!options.value.disabled === options.value.disabled ? options.value.disabled : false;"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_normal_comparison() {
        assert!(run_on("const r = a === b;").is_empty());
    }

    #[test]
    fn allows_negated_result() {
        assert!(run_on("const r = !(a === b);").is_empty());
    }
}
