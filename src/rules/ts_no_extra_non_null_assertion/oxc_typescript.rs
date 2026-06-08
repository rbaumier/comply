//! OxcCheck backend for ts-no-extra-non-null-assertion.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSNonNullExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSNonNullExpression(expr) = node.kind() else { return };
        if !matches!(&expr.expression, Expression::TSNonNullExpression(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Extra non-null assertion — `x!!` is redundant, use `x!`.".into(),
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
    fn flags_double_bang_on_expression() {
        let diags = run_on("const x = value!!;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_single_non_null_assertion() {
        assert!(run_on("const x = value!;").is_empty());
    }


    #[test]
    fn allows_boolean_coercion() {
        assert!(run_on("const x = !!value;").is_empty());
    }


    #[test]
    fn flags_triple_bang() {
        let diags = run_on("const x = value!!!;");
        // triple produces nested non_null_expression nodes
        assert!(!diags.is_empty());
    }
}
