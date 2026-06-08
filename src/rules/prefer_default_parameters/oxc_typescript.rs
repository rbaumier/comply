//! prefer-default-parameters OXC backend — flag `x = x || 'default'` / `x = x ?? 'default'`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression, LogicalOperator};
use std::sync::Arc;

pub struct Check;

fn is_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::StringLiteral(_)
            | Expression::TemplateLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
    ) || matches!(expr, Expression::Identifier(id) if id.name.as_str() == "undefined")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else {
            return;
        };

        // Left must be a simple identifier.
        let AssignmentTarget::AssignmentTargetIdentifier(left_id) = &assign.left else {
            return;
        };
        let lhs_name = left_id.name.as_str();

        // Right must be a logical expression with `||` or `??`.
        let Expression::LogicalExpression(logical) = &assign.right else {
            return;
        };
        if logical.operator != LogicalOperator::Or && logical.operator != LogicalOperator::Coalesce {
            return;
        }

        // Left side of || / ?? must be the same identifier.
        let Expression::Identifier(rl) = &logical.left else {
            return;
        };
        if rl.name.as_str() != lhs_name {
            return;
        }

        // Right side must be a literal.
        if !is_literal(&logical.right) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer default parameters over reassignment.".into(),
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
    fn flags_logical_or_reassignment() {
        let d = run_on("function f(x) {\n  x = x || 'default';\n}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-default-parameters");
    }


    #[test]
    fn flags_nullish_coalescing_reassignment() {
        let d = run_on("function f(x) {\n  x = x ?? 42;\n}");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_default_parameter() {
        assert!(run_on("function f(x = 'default') {}").is_empty());
    }


    #[test]
    fn allows_different_identifiers() {
        assert!(run_on("function f(x) {\n  x = y || 'default';\n}").is_empty());
    }


    #[test]
    fn allows_non_literal_rhs() {
        assert!(run_on("function f(x) {\n  x = x || getValue();\n}").is_empty());
    }
}
