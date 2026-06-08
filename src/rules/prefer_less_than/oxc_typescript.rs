//! prefer-less-than oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

/// RHS expressions that indicate a variable-vs-literal comparison
/// (e.g. `x > 0`, `arr.length >= 1`) which should not be flagged.
fn is_literal_rhs(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::UnaryExpression(_)
    ) || matches!(expr, Expression::Identifier(id) if id.name.as_str() == "undefined")
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
        let AstKind::BinaryExpression(bin) = node.kind() else {
            return;
        };

        let suggested = match bin.operator {
            BinaryOperator::GreaterThan => "<",
            BinaryOperator::GreaterEqualThan => "<=",
            _ => return,
        };

        let op = match bin.operator {
            BinaryOperator::GreaterThan => ">",
            BinaryOperator::GreaterEqualThan => ">=",
            _ => return,
        };

        if is_literal_rhs(&bin.right) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `{suggested}` over `{op}` for readability — swap operands and use `{suggested}`."
            ),
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
    fn flags_greater_than() {
        let d = run_on("const r = b > a;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<`"));
    }


    #[test]
    fn flags_greater_or_equal() {
        let d = run_on("const r = b >= a;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<=`"));
    }


    #[test]
    fn allows_variable_vs_literal() {
        assert!(run_on("if (x > 0) { f(); }").is_empty());
        assert!(run_on("if (arr.length >= 1) { f(); }").is_empty());
        assert!(run_on("const ok = count > 5;").is_empty());
    }


    #[test]
    fn allows_less_than() {
        assert!(run_on("const r = a < b;").is_empty());
    }


    #[test]
    fn allows_less_or_equal() {
        assert!(run_on("const r = a <= b;").is_empty());
    }


    #[test]
    fn allows_equality() {
        assert!(run_on("const r = a === b;").is_empty());
    }
}
