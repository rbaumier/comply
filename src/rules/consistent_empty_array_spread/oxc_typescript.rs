//! OXC backend for consistent-empty-array-spread.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SpreadElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SpreadElement(spread) = node.kind() else { return };

        // If the spread argument is a conditional (ternary), it's unparenthesized.
        // A parenthesized ternary would be wrapped in ParenthesizedExpression.
        if !matches!(spread.argument, Expression::ConditionalExpression(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, spread.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Parenthesize the ternary in array spread: \
                      `[...(condition ? ['a'] : [])]`.".into(),
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
    fn flags_unparenthesized_ternary_spread() {
        assert_eq!(run_on("const arr = [...condition ? ['a'] : []];").len(), 1);
    }


    #[test]
    fn allows_parenthesized_ternary_spread() {
        assert!(run_on("const arr = [...(condition ? ['a'] : [])];").is_empty());
    }


    #[test]
    fn flags_complex_condition() {
        assert_eq!(run_on("const arr = [...a && b ? [1] : []];").len(), 1);
    }


    #[test]
    fn allows_normal_spread() {
        assert!(run_on("const arr = [...items];").is_empty());
    }


    #[test]
    fn allows_optional_chaining_spread() {
        assert!(run_on("const arr = [...obj?.items];").is_empty());
    }
}
