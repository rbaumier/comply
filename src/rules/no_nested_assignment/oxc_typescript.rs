//! no-nested-assignment oxc backend — flag assignments inside conditions.
//!
//! A bare in-condition assignment (`if (x = 5)`) is flagged as a likely
//! `=`/`==` typo. An assignment wrapped in its own extra parentheses
//! (`if ((x = 5))`) or used as the operand of a comparison (`(x = e) !== y`)
//! is the deliberate assign-then-test idiom (ESLint `no-cond-assign`
//! `"except-parens"`) and is NOT flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement, AstType::WhileStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let test_expr = match node.kind() {
            AstKind::IfStatement(stmt) => &stmt.test,
            AstKind::WhileStatement(stmt) => &stmt.test,
            _ => return,
        };

        if contains_assignment(test_expr) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, test_expr.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Assignment inside a condition — likely a bug, use `===` for comparison or move the assignment out.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

fn is_comparison_operator(op: BinaryOperator) -> bool {
    matches!(
        op,
        BinaryOperator::Equality
            | BinaryOperator::Inequality
            | BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality
    )
}

/// Detect an in-condition assignment that is a likely `=`/`==` typo.
///
/// A bare assignment (`if (x = 5)`) is flagged. An assignment wrapped in its
/// own extra parentheses (`if ((x = 5))`) or used as a comparison operand
/// (`(x = e) !== y`) is the deliberate assign-then-test idiom and is NOT
/// flagged. Note: oxc preserves author-written parens as a
/// `ParenthesizedExpression` node, whereas the `if (...)` / `while (...)`
/// syntactic parens are not — so the bare typo never carries that wrapper.
fn contains_assignment(expr: &Expression) -> bool {
    match expr {
        Expression::AssignmentExpression(_) => true,
        Expression::ParenthesizedExpression(paren) => {
            // An assignment in its OWN extra parens is the deliberate idiom.
            if matches!(paren.expression, Expression::AssignmentExpression(_)) {
                return false;
            }
            contains_assignment(&paren.expression)
        }
        Expression::SequenceExpression(seq) => seq.expressions.iter().any(contains_assignment),
        Expression::LogicalExpression(log) => {
            contains_assignment(&log.left) || contains_assignment(&log.right)
        }
        Expression::BinaryExpression(bin) => {
            if is_comparison_operator(bin.operator) {
                // A comparison's assignment operand (`(x = e) !== y`) is the
                // deliberate compare-of-assignment, not a `=`/`==` confusion.
                let operand_flags = |e: &Expression| {
                    if matches!(
                        crate::oxc_helpers::peel_parens(e),
                        Expression::AssignmentExpression(_)
                    ) {
                        false
                    } else {
                        contains_assignment(e)
                    }
                };
                operand_flags(&bin.left) || operand_flags(&bin.right)
            } else {
                contains_assignment(&bin.left) || contains_assignment(&bin.right)
            }
        }
        Expression::ConditionalExpression(cond) => {
            contains_assignment(&cond.test)
                || contains_assignment(&cond.consequent)
                || contains_assignment(&cond.alternate)
        }
        _ => false,
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_rule;

    // Regression #3815: the explicitly-parenthesized assign-then-test idiom
    // (echarts) must NOT be flagged.
    #[test]
    fn allows_parenthesized_assignment_in_while_comparison() {
        assert!(run_rule(&Check, "while ((dateNum = getDate()) !== endDateNum) {}", "t.ts").is_empty());
    }

    #[test]
    fn allows_parenthesized_assignment_in_if_comparison() {
        assert!(run_rule(&Check, "if ((result = f()) != null) {}", "t.ts").is_empty());
    }

    #[test]
    fn allows_parenthesized_assignment_as_while_test() {
        assert!(run_rule(&Check, "while ((item = arr[i++])) {}", "t.ts").is_empty());
    }

    #[test]
    fn allows_parenthesized_assignment_under_logical_and() {
        assert!(run_rule(&Check, "if (a && (b = c())) {}", "t.ts").is_empty());
    }

    // Load-bearing guards: the bare typo must still be flagged.
    #[test]
    fn flags_bare_assignment_in_if() {
        assert_eq!(run_rule(&Check, "if (x = 5) {}", "t.ts").len(), 1);
    }

    #[test]
    fn flags_bare_assignment_in_while() {
        assert_eq!(run_rule(&Check, "while (x = next()) {}", "t.ts").len(), 1);
    }
}
