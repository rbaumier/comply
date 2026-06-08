use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

fn is_indexof_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    member.property.name.as_str() == "indexOf"
}

/// Returns "0", "-1" etc. as a static string for the common numeric comparisons.
fn numeric_literal_value(expr: &Expression) -> Option<&'static str> {
    match expr {
        Expression::NumericLiteral(n) => {
            if n.value == 0.0 { Some("0") }
            else if n.value == 1.0 { Some("1") }
            else { None }
        }
        Expression::UnaryExpression(u) => {
            if u.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
                && let Expression::NumericLiteral(n) = &u.argument
                    && n.value == 1.0 {
                        return Some("-1");
                    }
            None
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["indexOf"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        let compare_expr = if is_indexof_call(&bin.left) {
            &bin.right
        } else if is_indexof_call(&bin.right) {
            &bin.left
        } else {
            return;
        };

        let compare_text = numeric_literal_value(compare_expr);

        let op = bin.operator;
        let suggestion = match (op, compare_text) {
            (
                BinaryOperator::StrictEquality
                | BinaryOperator::Equality
                | BinaryOperator::StrictInequality
                | BinaryOperator::Inequality
                | BinaryOperator::GreaterEqualThan
                | BinaryOperator::GreaterThan,
                Some("-1"),
            ) => "includes()",
            (BinaryOperator::StrictEquality | BinaryOperator::Equality, Some("0")) => {
                "startsWith()"
            }
            _ => return,
        };

        // Match TreeSitter: >= 0 is not flagged (only >= -1, > -1)
        if matches!(
            op,
            BinaryOperator::GreaterEqualThan
        ) && compare_text == Some("-1")
        {
            "includes()";
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-indexof-equality".into(),
            message: format!("Use `{suggestion}` instead of `indexOf()` comparison."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(code, &Check)
    }


    #[test]
    fn flags_indexof_not_minus_one() {
        assert_eq!(run("str.indexOf('x') !== -1").len(), 1);
    }


    #[test]
    fn flags_indexof_equals_zero() {
        assert_eq!(run("str.indexOf('x') === 0").len(), 1);
    }


    #[test]
    fn flags_indexof_gte_zero() {
        assert_eq!(run("arr.indexOf(item) >= 0").len(), 0); // Not a common pattern we flag
    }


    #[test]
    fn flags_indexof_gt_minus_one() {
        assert_eq!(run("arr.indexOf(item) > -1").len(), 1);
    }


    #[test]
    fn allows_includes() {
        assert!(run("str.includes('x')").is_empty());
    }


    #[test]
    fn allows_starts_with() {
        assert!(run("str.startsWith('x')").is_empty());
    }
}
