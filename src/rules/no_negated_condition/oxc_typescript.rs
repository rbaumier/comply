//! no-negated-condition OxcCheck backend — flag negated conditions in
//! if/else (only when the negated branch is at least as large as the else)
//! and ternaries.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement, AstType::ConditionalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            oxc_ast::AstKind::IfStatement(stmt) => {
                // Must have an else branch.
                let Some(ref alt) = stmt.alternate else {
                    return;
                };
                // Skip `else if` chains.
                if matches!(alt, Statement::IfStatement(_)) {
                    return;
                }
                if is_negated_expr(&stmt.test)
                    && branch_size(&stmt.consequent) >= branch_size(alt)
                {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, stmt.test.span().start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Unexpected negated condition — swap the if/else branches \
                                  and remove the negation."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            oxc_ast::AstKind::ConditionalExpression(expr) => {
                if is_negated_expr(&expr.test) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, expr.test.span().start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Unexpected negated condition — swap the ternary arms \
                                  and remove the negation."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
        let _ = semantic;
    }
}

/// Number of statements in an `if`/`else` branch — a block's length, or 1 for
/// a single unbraced statement. The rule fires only when the negated (`if`)
/// branch carries at least as much code as the `else`: that is the case where
/// the main logic sits under the negation and swapping clarifies intent. A
/// small negated branch with a larger `else` reads as a guard, not a win.
fn branch_size(stmt: &Statement) -> usize {
    match stmt {
        Statement::BlockStatement(block) => block.body.len(),
        _ => 1,
    }
}

/// A condition is "negated" if it is:
/// - a `!expr` unary expression, OR
/// - a `!=` / `!==` binary expression.
fn is_negated_expr(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::UnaryExpression(u) => u.operator == UnaryOperator::LogicalNot,
        Expression::BinaryExpression(b) => {
            matches!(
                b.operator,
                BinaryOperator::Inequality | BinaryOperator::StrictInequality
            )
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
    fn flags_negated_if_else() {
        let d = run_on("if (!x) { a(); } else { b(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("swap the if/else"));
    }

    #[test]
    fn flags_not_equal_if_else() {
        let d = run_on("if (a !== b) { x(); } else { y(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_loose_not_equal_if_else() {
        let d = run_on("if (a != b) { x(); } else { y(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_if_without_else() {
        assert!(run_on("if (!x) { a(); }").is_empty());
    }

    #[test]
    fn allows_negated_if_smaller_than_else() {
        // Small negated guard, larger else — swapping would bury the main
        // branch under the negation rather than clarify it.
        assert!(run_on("if (!x) { return; } else { a(); b(); c(); }").is_empty());
    }

    #[test]
    fn flags_negated_if_larger_than_else() {
        let d = run_on("if (!x) { a(); b(); c(); } else { d(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_else_if() {
        assert!(run_on("if (!x) { a(); } else if (y) { b(); }").is_empty());
    }

    #[test]
    fn allows_positive_condition() {
        assert!(run_on("if (x) { a(); } else { b(); }").is_empty());
    }

    #[test]
    fn flags_negated_ternary() {
        let d = run_on("const r = !x ? a : b;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("swap the ternary"));
    }

    #[test]
    fn flags_not_equal_ternary() {
        let d = run_on("const r = a !== b ? x : y;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_positive_ternary() {
        assert!(run_on("const r = x ? a : b;").is_empty());
    }
}
