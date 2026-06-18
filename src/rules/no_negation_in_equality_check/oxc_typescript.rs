use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, UnaryOperator};
use std::sync::Arc;

pub struct Check;

const EQUALITY_OPS: &[BinaryOperator] = &[
    BinaryOperator::StrictEquality,
    BinaryOperator::StrictInequality,
    BinaryOperator::Equality,
    BinaryOperator::Inequality,
];

fn negate_op(op: BinaryOperator) -> &'static str {
    match op {
        BinaryOperator::StrictEquality => "!==",
        BinaryOperator::StrictInequality => "===",
        BinaryOperator::Equality => "!=",
        BinaryOperator::Inequality => "==",
        _ => "?",
    }
}

fn op_str(op: BinaryOperator) -> &'static str {
    match op {
        BinaryOperator::StrictEquality => "===",
        BinaryOperator::StrictInequality => "!==",
        BinaryOperator::Equality => "==",
        BinaryOperator::Inequality => "!=",
        _ => "?",
    }
}

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
        if !EQUALITY_OPS.contains(&bin.operator) {
            return;
        }

        let Expression::UnaryExpression(unary) = &bin.left else { return };
        if unary.operator != UnaryOperator::LogicalNot {
            return;
        }

        // Exclude double-negation `!!x === y`.
        if let Expression::UnaryExpression(inner) = &unary.argument
            && inner.operator == UnaryOperator::LogicalNot {
                return;
            }

        // `!a === !b` compares the truthiness of both operands — intentional,
        // no precedence surprise. Skip symmetric negation.
        if let Expression::UnaryExpression(right_unary) = &bin.right
            && right_unary.operator == UnaryOperator::LogicalNot {
                return;
            }

        let op = op_str(bin.operator);
        let neg_op = negate_op(bin.operator);
        let (line, column) = byte_offset_to_line_col(ctx.source, unary.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Negated expression in equality check: `!x {op} y` is `(!x) {op} y`. \
                 Use `x {neg_op} y` or `!(x {op} y)` instead.",
            ),
            severity: Severity::Error,
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
    fn flags_bang_strict_equals() {
        let d = run_on("if (!x === true) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!x === y"));
    }

    #[test]
    fn flags_bang_loose_equals() {
        assert_eq!(run_on("if (!x == true) {}").len(), 1);
    }

    #[test]
    fn flags_bang_strict_not_equals() {
        assert_eq!(run_on("if (!x !== false) {}").len(), 1);
    }

    #[test]
    fn flags_bang_loose_not_equals() {
        assert_eq!(run_on("if (!x != false) {}").len(), 1);
    }

    #[test]
    fn allows_double_negation() {
        assert!(run_on("if (!!x === true) {}").is_empty());
    }

    #[test]
    fn allows_normal_equality() {
        assert!(run_on("if (x === true) {}").is_empty());
    }

    #[test]
    fn allows_negation_on_right() {
        assert!(run_on("if (x === !y) {}").is_empty());
    }

    #[test]
    fn allows_symmetric_negation_strict_equals() {
        assert!(run_on("const eq = (a, b) => !a === !b;").is_empty());
    }

    #[test]
    fn allows_symmetric_negation_strict_not_equals() {
        assert!(run_on("const eq = (a, b) => !a !== !b;").is_empty());
    }

    #[test]
    fn flags_single_left_negation_with_plain_right() {
        assert_eq!(run_on("const eq = (a, b) => !a === b;").len(), 1);
    }
}
