//! ts-no-non-null-asserted-nullish-coalescing OXC backend — flag
//! `x! ?? y` where TSNonNullExpression is the left operand of a `??`
//! logical expression.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalOperator};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LogicalExpression(logical) = node.kind() else {
            return;
        };

        if logical.operator != LogicalOperator::Coalesce {
            return;
        }

        let Expression::TSNonNullExpression(non_null) = &logical.left else {
            return;
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, non_null.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`x! ?? y` is contradictory — the `!` asserts non-null \
                      while `??` handles null. Remove the `!`."
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
    fn flags_non_null_with_nullish_coalescing() {
        let diags = run_on("const x = value! ?? 'default';");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_nullish_coalescing_without_non_null() {
        assert!(run_on("const x = value ?? 'default';").is_empty());
    }


    #[test]
    fn allows_non_null_without_nullish_coalescing() {
        assert!(run_on("const x = value!;").is_empty());
    }
}
