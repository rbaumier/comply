//! prefer-promise-shorthand OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BindingPattern, Expression, FormalParameter, Statement,
};
use std::sync::Arc;

pub struct Check;

fn get_param_name<'a>(params: &'a [FormalParameter<'a>], index: usize) -> Option<&'a str> {
    let param = params.get(index)?;
    match &param.pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// Check if an expression is a call to an identifier with the given name.
fn is_call_to(expr: &Expression, name: &str) -> bool {
    if let Expression::CallExpression(call) = expr
        && let Expression::Identifier(id) = &call.callee {
            return id.name.as_str() == name;
        }
    false
}

fn matches_param(expr: &Expression, first: &str, second: Option<&str>) -> bool {
    is_call_to(expr, first) || second.is_some_and(|s| is_call_to(expr, s))
}

fn check_single_statement(stmt: &Statement, first: &str, second: Option<&str>) -> bool {
    match stmt {
        Statement::ExpressionStatement(e) => matches_param(&e.expression, first, second),
        Statement::ReturnStatement(ret) => {
            ret.argument.as_ref().is_some_and(|arg| matches_param(arg, first, second))
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new Promise"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };

        let Expression::Identifier(ctor) = &new_expr.callee else {
            return;
        };
        if ctor.name.as_str() != "Promise" {
            return;
        }

        if new_expr.arguments.len() != 1 {
            return;
        }

        let is_shorthand = match &new_expr.arguments[0] {
            Argument::ArrowFunctionExpression(arrow) => {
                let first = get_param_name(&arrow.params.items, 0);
                let second = get_param_name(&arrow.params.items, 1);
                let Some(first) = first else { return };

                if arrow.body.statements.len() != 1 {
                    return;
                }
                check_single_statement(&arrow.body.statements[0], first, second)
            }
            Argument::FunctionExpression(func) => {
                let first = get_param_name(&func.params.items, 0);
                let second = get_param_name(&func.params.items, 1);
                let Some(first) = first else { return };

                let Some(body) = &func.body else { return };
                if body.statements.len() != 1 {
                    return;
                }
                check_single_statement(&body.statements[0], first, second)
            }
            _ => return,
        };

        if !is_shorthand {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new Promise` wrapping a single resolve/reject — use `Promise.resolve()`/`Promise.reject()` instead.".into(),
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
    fn flags_promise_resolve_shorthand() {
        let d = run_on(r#"const p = new Promise((resolve) => resolve(42));"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-promise-shorthand");
    }


    #[test]
    fn flags_promise_reject_shorthand() {
        let d = run_on(r#"const p = new Promise((_, reject) => reject(new Error("fail")));"#);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_promise_with_logic() {
        let src = "const p = new Promise((resolve, reject) => {\n  fetchData().then(resolve).catch(reject);\n});";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_promise_resolve_static() {
        assert!(run_on("const p = Promise.resolve(42);").is_empty());
    }
}
