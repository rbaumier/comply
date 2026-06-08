//! no-nested-assignment oxc backend — flag assignments inside conditions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
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

fn contains_assignment(expr: &Expression) -> bool {
    match expr {
        Expression::AssignmentExpression(_) => true,
        Expression::ParenthesizedExpression(paren) => contains_assignment(&paren.expression),
        Expression::SequenceExpression(seq) => seq.expressions.iter().any(contains_assignment),
        Expression::LogicalExpression(log) => {
            contains_assignment(&log.left) || contains_assignment(&log.right)
        }
        Expression::BinaryExpression(bin) => {
            contains_assignment(&bin.left) || contains_assignment(&bin.right)
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
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_assignment_in_if() {
        assert_eq!(run_on("if (x = 10) {}").len(), 1);
    }


    #[test]
    fn flags_assignment_in_while() {
        assert_eq!(run_on("while (node = node.next) {}").len(), 1);
    }


    #[test]
    fn allows_equality_check() {
        assert!(run_on("if (x === 10) {}").is_empty());
    }


    #[test]
    fn allows_loose_equality() {
        assert!(run_on("if (x == 10) {}").is_empty());
    }


    #[test]
    fn allows_comparison_operators() {
        assert!(run_on("if (x <= 10) {}").is_empty());
        assert!(run_on("if (x >= 10) {}").is_empty());
    }
}
