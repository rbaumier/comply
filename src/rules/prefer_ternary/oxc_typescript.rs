//! prefer-ternary OXC backend — flag simple if/else that can be ternaries.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::IfStatement(if_stmt) = node.kind() else {
            return;
        };

        // Skip if this is an `else if` — parent is an IfStatement's alternate.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(parent.kind(), AstKind::IfStatement(_)) {
            // This if is the alternate of a parent if statement.
            return;
        }

        let Some(alternate) = &if_stmt.alternate else {
            return;
        };

        // Must not be an else-if chain.
        if matches!(alternate, Statement::IfStatement(_)) {
            return;
        }

        let cons_inner = single_statement(&if_stmt.consequent);
        let alt_inner = single_statement(alternate);

        let (cons_inner, alt_inner) = match (cons_inner, alt_inner) {
            (Some(c), Some(a)) => (c, a),
            _ => return,
        };

        // Case 1: both are expression statements with assignments to the same target.
        if let (
            Statement::ExpressionStatement(cons_expr_stmt),
            Statement::ExpressionStatement(alt_expr_stmt),
        ) = (cons_inner, alt_inner)
        {
            let cons_expr = &cons_expr_stmt.expression;
            let alt_expr = &alt_expr_stmt.expression;

            if let (
                Expression::AssignmentExpression(cons_assign),
                Expression::AssignmentExpression(alt_assign),
            ) = (cons_expr, alt_expr)
            {
                // Both must use the same operator.
                if cons_assign.operator != alt_assign.operator {
                    return;
                }

                let cons_lhs = &ctx.source[cons_assign.left.span().start as usize..cons_assign.left.span().end as usize];
                let alt_lhs = &ctx.source[alt_assign.left.span().start as usize..alt_assign.left.span().end as usize];

                if cons_lhs.trim() != alt_lhs.trim() || cons_lhs.trim().is_empty() {
                    return;
                }

                let op_display = &ctx.source[cons_assign.span.start as usize..cons_assign.span.end as usize];
                let op_str = if cons_assign.operator == AssignmentOperator::Assign {
                    "="
                } else {
                    // Extract the operator text
                    operator_str(cons_assign.operator)
                };

                let (line, column) = byte_offset_to_line_col(ctx.source, if_stmt.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "prefer-ternary".into(),
                    message: format!(
                        "This `if` statement can be replaced by a ternary: \
                         `{lhs} {op} cond ? consequent : alternate`.",
                        lhs = cons_lhs.trim(),
                        op = op_str,
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                let _ = op_display;
                return;
            }
            return;
        }

        // Case 2: both are return statements.
        if matches!(cons_inner, Statement::ReturnStatement(_))
            && matches!(alt_inner, Statement::ReturnStatement(_))
        {
            let (line, column) = byte_offset_to_line_col(ctx.source, if_stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "prefer-ternary".into(),
                message: "This `if` statement can be replaced by `return cond ? a : b;`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Extract the single meaningful statement from a block or bare statement.
fn single_statement<'a>(stmt: &'a Statement<'a>) -> Option<&'a Statement<'a>> {
    if let Statement::BlockStatement(block) = stmt {
        let meaningful: Vec<&Statement> = block
            .body
            .iter()
            .filter(|s| !matches!(s, Statement::EmptyStatement(_)))
            .collect();
        if meaningful.len() == 1 {
            return Some(meaningful[0]);
        }
        None
    } else {
        match stmt {
            Statement::ExpressionStatement(_) | Statement::ReturnStatement(_) => Some(stmt),
            _ => None,
        }
    }
}

fn operator_str(op: AssignmentOperator) -> &'static str {
    match op {
        AssignmentOperator::Assign => "=",
        AssignmentOperator::Addition => "+=",
        AssignmentOperator::Subtraction => "-=",
        AssignmentOperator::Multiplication => "*=",
        AssignmentOperator::Division => "/=",
        AssignmentOperator::Remainder => "%=",
        AssignmentOperator::Exponential => "**=",
        AssignmentOperator::ShiftLeft => "<<=",
        AssignmentOperator::ShiftRight => ">>=",
        AssignmentOperator::ShiftRightZeroFill => ">>>=",
        AssignmentOperator::BitwiseAnd => "&=",
        AssignmentOperator::BitwiseOR => "|=",
        AssignmentOperator::BitwiseXOR => "^=",
        AssignmentOperator::LogicalAnd => "&&=",
        AssignmentOperator::LogicalOr => "||=",
        AssignmentOperator::LogicalNullish => "??=",
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_simple_assignment_if_else() {
        let d = run_on("if (cond) { x = a; } else { x = b; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("ternary"));
    }


    #[test]
    fn flags_return_if_else() {
        let d = run_on("function f() { if (cond) { return a; } else { return b; } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return"));
    }


    #[test]
    fn allows_different_targets() {
        assert!(run_on("if (c) { x = 1; } else { y = 2; }").is_empty());
    }


    #[test]
    fn allows_multi_statement_branches() {
        assert!(run_on("if (c) { x = 1; log(); } else { x = 2; }").is_empty());
    }


    #[test]
    fn allows_if_without_else() {
        assert!(run_on("if (c) { x = 1; }").is_empty());
    }


    #[test]
    fn allows_else_if_chain() {
        assert!(run_on("if (a) { x = 1; } else if (b) { x = 2; } else { x = 3; }").is_empty());
    }


    #[test]
    fn flags_compound_assignment() {
        let d = run_on("if (cond) { x += a; } else { x += b; }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn rejects_different_operators() {
        // `=` vs `+=` are different node kinds, so they don't match.
        assert!(run_on("if (c) { x = 1; } else { x += 2; }").is_empty());
    }


    #[test]
    fn rejects_different_augmented_operators() {
        // `+=` vs `-=` are both augmented but different operators.
        assert!(run_on("if (c) { x += 1; } else { x -= 2; }").is_empty());
    }
}
