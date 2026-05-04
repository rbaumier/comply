use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::ExpressionStatement(stmt) = node.kind() else {
                continue;
            };

            let expr = &stmt.expression;

            // String literals in expression position are allowed (directive prologues)
            if matches!(expr, Expression::StringLiteral(_) | Expression::TemplateLiteral(_)) {
                continue;
            }

            if has_side_effects(expr) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Expected an assignment or function call, got an expression with no side effects.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

fn has_side_effects(expr: &Expression) -> bool {
    match expr {
        // Always side-effectful
        Expression::CallExpression(_)
        | Expression::NewExpression(_)
        | Expression::AwaitExpression(_)
        | Expression::YieldExpression(_)
        | Expression::AssignmentExpression(_)
        | Expression::UpdateExpression(_)
        | Expression::TaggedTemplateExpression(_) => true,

        // Unary: only delete/void are side-effectful
        Expression::UnaryExpression(unary) => {
            use oxc_ast::ast::UnaryOperator;
            matches!(
                unary.operator,
                UnaryOperator::Delete | UnaryOperator::Void
            )
        }

        // Short-circuit: allowed if RHS has side effects
        Expression::LogicalExpression(logic) => has_side_effects(&logic.right),

        // Ternary: allowed if both branches have side effects
        Expression::ConditionalExpression(cond) => {
            has_side_effects(&cond.consequent) && has_side_effects(&cond.alternate)
        }

        // Sequence: last expression matters
        Expression::SequenceExpression(seq) => {
            seq.expressions.last().is_some_and(|e| has_side_effects(e))
        }

        // Parenthesized
        Expression::ParenthesizedExpression(paren) => has_side_effects(&paren.expression),

        // TS non-null assertion: unwrap
        Expression::TSNonNullExpression(inner) => has_side_effects(&inner.expression),

        // TS `as` / `satisfies`: unwrap
        Expression::TSAsExpression(inner) => has_side_effects(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => has_side_effects(&inner.expression),

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
    fn flags_bare_identifier() {
        let d = run_on("let x = 1; x;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_function_call() {
        assert!(run_on("console.log('hello');").is_empty());
    }

    #[test]
    fn allows_assignment() {
        assert!(run_on("let x = 1; x = 2;").is_empty());
    }

    #[test]
    fn flags_bare_arithmetic() {
        let d = run_on("1 + 2;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_short_circuit_with_call() {
        assert!(run_on("const x = true; x && console.log('y');").is_empty());
    }
}
