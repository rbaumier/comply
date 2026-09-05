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
        // In a tsd / expect-type type-test file the `(() => (<JSX/>))()`
        // arrow-IIFE is the idiomatic way to evaluate a JSX expression as a
        // statement for type-checking.
        if ctx.file.is_type_test_file() {
            return;
        }

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
                    severity: Severity::Error,
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Builds the real `FileCtx` from `path`, which the type-test-file guard reads.
    fn run_on(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, source, path)
    }

    #[test]
    fn flags_parenthesized_arrow_iife() {
        let d = run_on("const foo = (() => (bar))();", "t.ts");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unreadable-iife");
    }

    #[test]
    fn allows_arrow_iife_without_parens_body() {
        assert!(run_on("const foo = (() => bar)();", "t.ts").is_empty());
    }

    #[test]
    fn exempts_jsx_iife_in_test_d_tsx() {
        // tsd / expect-type type-check idiom from issue #2037: the arrow-IIFE
        // wrapping JSX is the only way to evaluate JSX as a statement.
        let src = "(() => (\n  <Component prop={1} />\n))();";
        assert!(
            run_on(src, "src/Component.test-d.tsx").is_empty(),
            "JSX-wrapping IIFE in a .test-d.tsx file must not be flagged"
        );
    }

    #[test]
    fn exempts_iife_in_every_type_test_file_convention() {
        // One shared spelling of "this is a tsd/dtslint type test", so a
        // convention recognised for one type-test rule is recognised for all.
        for path in [
            "src/types.test-d.ts",
            "src/types.types-test.ts",
            "test-d/types.ts",
            "test-tsd/types.ts",
            "dtslint/types.ts",
            "src/addDays/test.tp.ts",
        ] {
            assert!(
                run_on("const foo = (() => (bar))();", path).is_empty(),
                "IIFE in type-test file {path} must not be flagged"
            );
        }
    }

    #[test]
    fn still_flags_iife_in_ordinary_test_file() {
        // `.test.ts` (no `-d`) is a runtime test, not a type-declaration test:
        // an unreadable IIFE there is still a genuine finding.
        let d = run_on("const foo = (() => (bar))();", "src/foo.test.ts");
        assert_eq!(d.len(), 1);
    }
}
