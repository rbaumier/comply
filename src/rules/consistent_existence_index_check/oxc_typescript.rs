//! consistent-existence-index-check OXC backend — flag `< 0`, `>= 0`, `> -1`
//! on index method calls. Prefer `=== -1` / `!== -1`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, UnaryOperator};
use std::sync::Arc;

pub struct Check;

const INDEX_METHODS: &[&str] = &["indexOf", "lastIndexOf", "findIndex", "findLastIndex"];

fn is_index_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    INDEX_METHODS.contains(&member.property.name.as_str())
}

fn is_index_identifier(expr: &Expression) -> bool {
    let Expression::Identifier(id) = expr else { return false };
    let lower = id.name.as_str().to_ascii_lowercase();
    lower.contains("index") || lower.contains("idx")
}

fn is_index_expr(expr: &Expression) -> bool {
    is_index_call(expr) || is_index_identifier(expr)
}

fn is_zero(expr: &Expression) -> bool {
    matches!(expr, Expression::NumericLiteral(n) if n.value == 0.0)
}

fn is_negative_one(expr: &Expression) -> bool {
    if let Expression::UnaryExpression(u) = expr
        && u.operator == UnaryOperator::UnaryNegation
            && let Expression::NumericLiteral(n) = &u.argument {
                return n.value == 1.0;
            }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["indexOf", "lastIndexOf", "findIndex", "findLastIndex"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        let op = bin.operator;
        let is_bad = if (op == BinaryOperator::LessThan || op == BinaryOperator::GreaterEqualThan)
            && is_zero(&bin.right)
        {
            is_index_expr(&bin.left)
        } else if op == BinaryOperator::GreaterThan && is_negative_one(&bin.right) {
            is_index_expr(&bin.left)
        } else {
            false
        };

        if !is_bad {
            return;
        }

        let message = if op == BinaryOperator::LessThan {
            "Prefer `=== -1` over `< 0` to check index non-existence."
        } else {
            "Prefer `!== -1` over `>= 0` / `> -1` to check index existence."
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: message.into(),
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
    fn flags_inline_index_of_less_than_zero() {
        let d = run_on("if (foo.indexOf('bar') < 0) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("=== -1"));
    }


    #[test]
    fn flags_inline_index_of_gte_zero() {
        let d = run_on("if (foo.indexOf('bar') >= 0) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!== -1"));
    }


    #[test]
    fn flags_inline_index_of_gt_minus_one() {
        let d = run_on("if (foo.indexOf('bar') > -1) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!== -1"));
    }


    #[test]
    fn flags_find_last_index() {
        let d = run_on("if (arr.findLastIndex(x => x) > -1) {}");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_last_index_of() {
        let d = run_on("if (str.lastIndexOf('a') < 0) {}");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_triple_equals_minus_one() {
        assert!(run_on("if (foo.indexOf('bar') === -1) {}").is_empty());
    }


    #[test]
    fn allows_not_equals_minus_one() {
        assert!(run_on("if (foo.indexOf('bar') !== -1) {}").is_empty());
    }


    #[test]
    fn allows_unrelated_comparison() {
        assert!(run_on("if (count < 0) {}").is_empty());
    }
}
