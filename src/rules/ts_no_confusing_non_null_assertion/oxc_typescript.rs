//! ts-no-confusing-non-null-assertion oxc backend — flag `a! == b` which
//! looks confusingly like `a !== b`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

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
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        let is_confusing = matches!(
            bin.operator,
            BinaryOperator::Equality
                | BinaryOperator::StrictEquality
                | BinaryOperator::Instanceof
                | BinaryOperator::In
        );
        if !is_confusing {
            return;
        }

        if !matches!(&bin.left, Expression::TSNonNullExpression(_)) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, bin.left.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Confusing non-null assertion before comparison — \
                      `a! == b` looks like `a !== b`. Remove the `!` or \
                      wrap in parentheses."
                .into(),
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
    fn flags_non_null_before_equality() {
        let diags = run_on("const r = a! == b;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_non_null_before_strict_equality() {
        let diags = run_on("const r = a! === b;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_proper_not_equal() {
        assert!(run_on("const r = a !== b;").is_empty());
    }


    #[test]
    fn allows_proper_not_strict_equal() {
        assert!(run_on("const r = a != b;").is_empty());
    }
}
