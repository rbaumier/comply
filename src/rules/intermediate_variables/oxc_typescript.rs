//! intermediate-variables — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const LOGICAL_OPS: &[&str] = &["&&", "||", "??"];

fn count_logical_ops(expr: &Expression) -> usize {
    match expr {
        Expression::LogicalExpression(log) => {
            let op_str = log.operator.as_str();
            let is_logical = LOGICAL_OPS.contains(&op_str);
            let count = if is_logical { 1 } else { 0 };
            count + count_logical_ops(&log.left) + count_logical_ops(&log.right)
        }
        Expression::ParenthesizedExpression(paren) => count_logical_ops(&paren.expression),
        // Stop at callable boundaries — nested lambda predicates don't count.
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => 0,
        Expression::BinaryExpression(bin) => {
            count_logical_ops(&bin.left) + count_logical_ops(&bin.right)
        }
        Expression::UnaryExpression(un) => count_logical_ops(&un.argument),
        Expression::ConditionalExpression(cond) => {
            count_logical_ops(&cond.test)
                + count_logical_ops(&cond.consequent)
                + count_logical_ops(&cond.alternate)
        }
        Expression::CallExpression(call) => {
            // Don't descend into call arguments — they may contain lambdas.
            let mut n = count_logical_ops(&call.callee);
            for arg in &call.arguments {
                if let Some(e) = arg.as_expression() {
                    // Stop at arrow/function expression arguments.
                    match e {
                        Expression::ArrowFunctionExpression(_)
                        | Expression::FunctionExpression(_) => {}
                        _ => n += count_logical_ops(e),
                    }
                }
            }
            n
        }
        _ => 0,
    }
}

#[derive(Debug)]
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
        let oxc_ast::AstKind::IfStatement(if_stmt) = node.kind() else {
            return;
        };
        let min_ops = ctx.config.threshold("intermediate-variables", "min_ops", ctx.lang);
        if count_logical_ops(&if_stmt.test) < min_ops {
            return;
        }
        let span = if_stmt.test.span();
        let (line, col) = byte_offset_to_line_col(semantic.source_text(), span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: "`if` condition chains three or more boolean operands \u{2014} extract parts into named local variables.".into(),
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
    fn flags_three_operand_and_chain() {
        assert_eq!(run_on("if (a && b && c) { x(); }").len(), 1);
    }


    #[test]
    fn flags_four_operand_or_chain() {
        assert_eq!(run_on("if (a || b || c || d) { x(); }").len(), 1);
    }


    #[test]
    fn flags_nullish_coalesce_chain() {
        assert_eq!(run_on("if (a ?? b ?? c) { x(); }").len(), 1);
    }


    #[test]
    fn allows_two_operand_and() {
        assert!(run_on("if (a && b) { x(); }").is_empty());
    }


    #[test]
    fn allows_single_condition() {
        assert!(run_on("if (a) { x(); }").is_empty());
    }


    #[test]
    fn allows_condition_with_comparisons_only() {
        assert!(run_on("if (a === 1 && b === 2) { x(); }").is_empty());
    }


    #[test]
    fn allows_long_expression_inside_comparison_chain() {
        assert!(run_on("if (a + b * c / d === e) { x(); }").is_empty());
    }


    #[test]
    fn does_not_flag_call_with_complex_arg() {
        // The outer node is a call_expression; the rule never inspects
        // calls at all, so complex arguments don't matter.
        assert!(run_on("doSomething(a + b * c / d);").is_empty());
    }


    #[test]
    fn closure_predicate_inside_condition_does_not_count() {
        // `.some(x => x.a && x.b && x.c)` is a lambda inside the call
        // argument. The walk stops at the arrow_function so its
        // operators don't contribute to the enclosing if's count.
        let src = "if (items.some(x => x.a && x.b && x.c && x.d)) { go(); }";
        assert!(run_on(src).is_empty());
    }
}
