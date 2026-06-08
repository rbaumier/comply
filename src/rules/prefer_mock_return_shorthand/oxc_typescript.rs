//! prefer-mock-return-shorthand oxc backend.
//!
//! Flag `.mockImplementation(() => value)` where the callback just returns
//! a value, suggest `.mockReturnValue(value)` instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

/// Return true if `expr` is `Promise.resolve(...)` or `Promise.reject(...)`.
fn is_promise_settle(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name.as_str() == "Promise"
        && matches!(
            member.property.name.as_str(),
            "resolve" | "reject"
        )
}

/// Extract the returned expression of a function whose body is a single
/// expression (arrow concise) or a block with a single `return`.
fn single_return_expr<'a>(
    body: &'a oxc_ast::ast::FunctionBody<'a>,
    is_expression: bool,
) -> Option<&'a Expression<'a>> {
    // Arrow concise body: the body has a single expression statement.
    if is_expression {
        if body.statements.len() == 1
            && let Statement::ExpressionStatement(es) = &body.statements[0] {
                return Some(&es.expression);
            }
        return None;
    }

    // Block body: must have exactly one return statement (ignoring comments,
    // which oxc doesn't represent as statements).
    let mut return_expr = None;
    for stmt in &body.statements {
        match stmt {
            Statement::ReturnStatement(ret) => {
                if return_expr.is_some() {
                    return None;
                }
                return_expr = ret.argument.as_ref();
            }
            _ => return None,
        }
    }
    return_expr
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["mockImplementation"])
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

        // Callee must be `*.mockImplementation`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "mockImplementation" {
            return;
        }

        // Exactly one argument.
        if call.arguments.len() != 1 {
            return;
        }
        let Some(arg_expr) = call.arguments[0].as_expression() else {
            return;
        };

        let expr = match arg_expr {
            Expression::ArrowFunctionExpression(arrow) => {
                single_return_expr(&arrow.body, arrow.expression)
            }
            Expression::FunctionExpression(func) => {
                func.body.as_ref().and_then(|b| single_return_expr(b, false))
            }
            _ => None,
        };

        let Some(expr) = expr else {
            return;
        };

        if is_promise_settle(expr) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `.mockReturnValue(x)` over `.mockImplementation(() => x)`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_arrow_expression_body_literal() {
        assert_eq!(run("fn.mockImplementation(() => 42);").len(), 1);
    }

    #[test]
    fn flags_arrow_block_body_single_return() {
        assert_eq!(run("fn.mockImplementation(() => { return 42; });").len(), 1);
    }

    #[test]
    fn flags_function_expression_single_return() {
        assert_eq!(
            run("fn.mockImplementation(function () { return value; });").len(),
            1
        );
    }

    #[test]
    fn flags_arrow_returning_object() {
        assert_eq!(run("fn.mockImplementation(() => ({ id: 1 }));").len(), 1);
    }

    #[test]
    fn allows_mock_return_value_shorthand() {
        assert!(run("fn.mockReturnValue(42);").is_empty());
    }

    #[test]
    fn allows_implementation_with_logic() {
        let src = "fn.mockImplementation(() => { doWork(); return 1; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_implementation_with_multiple_statements() {
        let src = "fn.mockImplementation(() => { const x = compute(); return x + 1; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_promise_resolve_body() {
        assert!(run("fn.mockImplementation(() => Promise.resolve(1));").is_empty());
    }

    #[test]
    fn skips_promise_reject_body() {
        assert!(run("fn.mockImplementation(() => Promise.reject(new Error('x')));").is_empty());
    }

    #[test]
    fn allows_unrelated_call() {
        assert!(run("foo(() => 42);").is_empty());
    }
}
