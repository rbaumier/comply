//! no-unreadable-iife OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // The callee must be an arrow function (possibly wrapped in parens).
        let callee = unwrap_parens(&call.callee);
        let oxc_ast::ast::Expression::ArrowFunctionExpression(arrow) = callee else {
            return;
        };

        // Block body is fine (normal multi-statement IIFE).
        if arrow.expression {
            // expression = true means concise body (not block).
            // Check if the body's single expression is parenthesized.
            // In OXC, parenthesized expressions are represented with
            // `ParenthesizedExpression`.
            let Some(stmt) = arrow.body.statements.first() else {
                return;
            };
            let oxc_ast::ast::Statement::ExpressionStatement(expr_stmt) = stmt else {
                return;
            };
            if matches!(
                &expr_stmt.expression,
                oxc_ast::ast::Expression::ParenthesizedExpression(_)
            ) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message:
                        "IIFE with parenthesized arrow function body is considered unreadable."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

fn unwrap_parens<'a>(expr: &'a oxc_ast::ast::Expression<'a>) -> &'a oxc_ast::ast::Expression<'a> {
    let mut current = expr;
    while let oxc_ast::ast::Expression::ParenthesizedExpression(paren) = current {
        current = &paren.expression;
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_parenthesized_arrow_iife() {
        let d = run_on("const foo = (() => (bar))();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unreadable-iife");
    }


    #[test]
    fn flags_multiline_parenthesized_arrow_iife() {
        let d = run_on("const foo = (() => (bar + baz))();");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_arrow_iife_without_parens_body() {
        // `(() => bar)()` — body is not parenthesized, fine.
        assert!(run_on("const foo = (() => bar)();").is_empty());
    }


    #[test]
    fn allows_arrow_iife_with_block_body() {
        // `(() => { return bar; })()` — block body, fine.
        assert!(run_on("const foo = (() => { return bar; })();").is_empty());
    }


    #[test]
    fn allows_regular_function_iife() {
        // `(function() { return 42; })()` — not an arrow function, fine.
        assert!(run_on("(function() { return 42; })();").is_empty());
    }


    #[test]
    fn allows_normal_call() {
        assert!(run_on("foo(bar);").is_empty());
    }
}
