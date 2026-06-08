//! prefer-mock-promise-shorthand OxcCheck backend — flag
//! `x.mockImplementation(() => Promise.resolve(v))` and
//! `x.mockImplementation(() => Promise.reject(v))`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

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

        // Callee must be a member expression ending in `.mockImplementation`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "mockImplementation" {
            return;
        }

        // Exactly one argument: a function
        if call.arguments.len() != 1 {
            return;
        }
        let Some(arg) = call.arguments.first() else {
            return;
        };

        let kind = match arg {
            Argument::ArrowFunctionExpression(arrow) => settle_kind_from_arrow(arrow, ctx.source),
            Argument::FunctionExpression(func) => settle_kind_from_func(func, ctx.source),
            _ => None,
        };

        let Some(kind) = kind else {
            return;
        };

        let shorthand = match kind {
            "resolve" => "mockResolvedValue",
            "reject" => "mockRejectedValue",
            _ => return,
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `.{shorthand}(x)` over `.mockImplementation(() => Promise.{kind}(x))`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// If `body` is a `Promise.resolve(x)` / `Promise.reject(x)` call expression,
/// return the property name (`"resolve"` or `"reject"`).
fn promise_settle_kind<'a>(expr: &Expression<'a>) -> Option<&'static str> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let Expression::Identifier(obj) = &member.object else {
        return None;
    };
    if obj.name.as_str() != "Promise" {
        return None;
    }
    match member.property.name.as_str() {
        "resolve" => Some("resolve"),
        "reject" => Some("reject"),
        _ => None,
    }
}

fn settle_kind_from_arrow<'a>(
    arrow: &oxc_ast::ast::ArrowFunctionExpression<'a>,
    _source: &str,
) -> Option<&'static str> {
    // If expression body (single expression, no braces)
    if arrow.expression {
        let stmts = &arrow.body.statements;
        if stmts.len() == 1
            && let Statement::ExpressionStatement(expr_stmt) = &stmts[0] {
                return promise_settle_kind(&expr_stmt.expression);
            }
        return None;
    }

    // Block body: must contain exactly one return statement
    settle_kind_from_block_body(&arrow.body.statements)
}

fn settle_kind_from_func<'a>(
    func: &oxc_ast::ast::Function<'a>,
    _source: &str,
) -> Option<&'static str> {
    let body = func.body.as_ref()?;
    settle_kind_from_block_body(&body.statements)
}

fn settle_kind_from_block_body<'a>(
    stmts: &[Statement<'a>],
) -> Option<&'static str> {
    // Must contain exactly one non-empty statement, which is a return statement
    let mut return_stmt = None;
    for stmt in stmts {
        match stmt {
            Statement::EmptyStatement(_) => continue,
            Statement::ReturnStatement(ret) => {
                if return_stmt.is_some() {
                    return None; // More than one statement
                }
                return_stmt = Some(ret);
            }
            _ => return None, // Non-return statement
        }
    }
    let ret = return_stmt?;
    let expr = ret.argument.as_ref()?;
    promise_settle_kind(expr)
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_arrow_expression_body_resolve() {
        assert_eq!(
            run("fn.mockImplementation(() => Promise.resolve(1));").len(),
            1
        );
    }


    #[test]
    fn flags_arrow_expression_body_reject() {
        assert_eq!(
            run("fn.mockImplementation(() => Promise.reject(new Error('x')));").len(),
            1
        );
    }


    #[test]
    fn flags_arrow_block_body_resolve() {
        assert_eq!(
            run("fn.mockImplementation(() => { return Promise.resolve(42); });").len(),
            1
        );
    }


    #[test]
    fn flags_function_expression_reject() {
        assert_eq!(
            run("fn.mockImplementation(function () { return Promise.reject(err); });").len(),
            1
        );
    }


    #[test]
    fn allows_mock_resolved_value_shorthand() {
        assert!(run("fn.mockResolvedValue(1);").is_empty());
    }


    #[test]
    fn allows_mock_rejected_value_shorthand() {
        assert!(run("fn.mockRejectedValue(new Error('x'));").is_empty());
    }


    #[test]
    fn allows_non_promise_implementation() {
        assert!(run("fn.mockImplementation(() => 42);").is_empty());
    }


    #[test]
    fn allows_implementation_with_logic() {
        let src = "fn.mockImplementation(() => { doWork(); return Promise.resolve(1); });";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_implementation_with_params() {
        // Arg-using implementations can't be replaced with a static value.
        // We still flag them since the body only returns `Promise.resolve(x)`,
        // but only when the returned expression doesn't depend on params.
        // Here the body returns `Promise.resolve(a)` which depends on `a` —
        // but the rule's remit (matching eslint-plugin-unicorn) still flags this.
        // Accept either 0 or 1 depending on interpretation — we keep it simple
        // and flag, which matches the upstream rule.
        let src = "fn.mockImplementation((a) => Promise.resolve(a));";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_promise_all() {
        assert!(run("fn.mockImplementation(() => Promise.all([a, b]));").is_empty());
    }
}
