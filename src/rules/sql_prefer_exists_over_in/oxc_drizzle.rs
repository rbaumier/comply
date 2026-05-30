use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if super::is_test_file(ctx.path) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "inArray" {
            return;
        }
        let Some(second) = call.arguments.get(1) else {
            return;
        };
        let second_src = &ctx.source[second.span().start as usize..second.span().end as usize];
        if !second_src.contains("select") || !second_src.contains("from") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`inArray(col, subquery)` — prefer `exists()` which \
                      short-circuits on first match."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_inarray_with_subquery() {
        assert_eq!(
            run_on("where(inArray(users.id, db.select({ id: orders.userId }).from(orders)));").len(),
            1
        );
    }

    #[test]
    fn allows_inarray_with_array_literal() {
        assert!(run_on("where(inArray(users.role, ['admin', 'editor']));").is_empty());
    }

    #[test]
    fn no_fp_in_test_file() {
        // Regression for #528: inArray(col, subquery) in test files is not a FP.
        let src = "where(inArray(users.id, db.select({ id: orders.userId }).from(orders)));";
        let diags = crate::rules::test_helpers::run_oxc_ts_with_path(
            src,
            &Check,
            "src/features/users/users.integration.test.ts",
        );
        assert!(diags.is_empty());
    }
}
